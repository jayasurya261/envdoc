use crate::model::evidence::EvidenceKind;
use crate::model::name::EnvName;
use crate::model::path::RelPath;
use crate::model::reference::Reference;
use crate::model::span::Span;
use crate::scan::ScanResult;

/// Scans infrastructure, container, and configuration files:
/// - Dockerfile (`ENV`, `ARG`)
/// - Shell scripts (`$VAR`, `${VAR}`)
/// - Docker Compose (`environment:`, `${VAR}`)
/// - GitHub Actions (`${{ env.VAR }}`, `${{ secrets.VAR }}`)
/// - Kubernetes manifests (`- name: VAR` under `env:`)
/// - Terraform (`var.VAR`, `env("VAR")`)
pub fn scan_infra_source(source_text: &str, path: impl Into<RelPath>) -> ScanResult {
    let rel_path = path.into();
    let file_str = rel_path.as_str().to_ascii_lowercase();

    if file_str.ends_with("dockerfile") || file_str.contains("dockerfile.") {
        scan_dockerfile(source_text, rel_path)
    } else if file_str.ends_with(".sh") || file_str.ends_with(".bash") || file_str.ends_with(".zsh")
    {
        scan_shell(source_text, rel_path)
    } else if file_str.ends_with(".tf") || file_str.ends_with(".tfvars") {
        scan_terraform(source_text, rel_path)
    } else {
        // YAML files: Docker compose, GitHub Actions, Kubernetes
        scan_yaml_infra(source_text, rel_path)
    }
}

fn scan_dockerfile(source_text: &str, path: RelPath) -> ScanResult {
    let mut references = Vec::new();

    for (idx, line) in source_text.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        let trimmed = line.trim();

        if trimmed.starts_with('#') {
            continue;
        }

        if let Some(rest) = trimmed
            .strip_prefix("ENV ")
            .or_else(|| trimmed.strip_prefix("ARG "))
        {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            for part in parts {
                let key = if let Some((k, _)) = part.split_once('=') {
                    k
                } else {
                    part
                };
                let env_name = EnvName::new(key);
                if env_name.is_valid_identifier() {
                    let col = (line.find(key).unwrap_or(0) + 1) as u32;
                    let span = Span::point(line_num, col, 0);
                    references.push(Reference::new(
                        env_name,
                        path.clone(),
                        span,
                        EvidenceKind::DirectMember,
                    ));
                }
            }
        }
    }

    ScanResult {
        references,
        diagnostics: Vec::new(),
    }
}

fn scan_shell(source_text: &str, path: RelPath) -> ScanResult {
    let mut references = Vec::new();

    for (idx, line) in source_text.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        let code_line = if let Some(i) = line.find('#') {
            &line[..i]
        } else {
            line
        };

        // Extract $VAR or ${VAR}
        let bytes = code_line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' && i + 1 < bytes.len() {
                let start_col = (i + 1) as u32;
                i += 1;
                if bytes[i] == b'{' {
                    i += 1;
                    let var_start = i;
                    while i < bytes.len()
                        && bytes[i] != b'}'
                        && bytes[i] != b':'
                        && bytes[i] != b'-'
                    {
                        i += 1;
                    }
                    if var_start < i {
                        let key = &code_line[var_start..i];
                        let env_name = EnvName::new(key);
                        if env_name.is_valid_identifier() {
                            references.push(Reference::new(
                                env_name,
                                path.clone(),
                                Span::point(line_num, start_col, 0),
                                EvidenceKind::DirectMember,
                            ));
                        }
                    }
                } else if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
                    let var_start = i;
                    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
                    {
                        i += 1;
                    }
                    let key = &code_line[var_start..i];
                    let env_name = EnvName::new(key);
                    if env_name.is_valid_identifier() {
                        references.push(Reference::new(
                            env_name,
                            path.clone(),
                            Span::point(line_num, start_col, 0),
                            EvidenceKind::DirectMember,
                        ));
                    }
                }
            } else {
                i += 1;
            }
        }
    }

    ScanResult {
        references,
        diagnostics: Vec::new(),
    }
}

fn scan_terraform(source_text: &str, path: RelPath) -> ScanResult {
    let mut references = Vec::new();

    for (idx, line) in source_text.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        // Search for env("VAR") or TF_VAR_
        if let Some(pos) = line.find("env(") {
            let after = &line[pos + 4..];
            if let Some(quote) = after.chars().next() {
                if quote == '"' || quote == '\'' {
                    if let Some(end_q) = after[1..].find(quote) {
                        let key = &after[1..1 + end_q];
                        let env_name = EnvName::new(key);
                        if env_name.is_valid_identifier() {
                            let col = (pos + 1) as u32;
                            references.push(Reference::new(
                                env_name,
                                path.clone(),
                                Span::point(line_num, col, 0),
                                EvidenceKind::DirectMember,
                            ));
                        }
                    }
                }
            }
        }
    }

    ScanResult {
        references,
        diagnostics: Vec::new(),
    }
}

fn scan_yaml_infra(source_text: &str, path: RelPath) -> ScanResult {
    let mut references = Vec::new();

    for (idx, line) in source_text.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        let trimmed = line.trim();

        // 1. Kubernetes: - name: VAR_NAME under env:
        if let Some(rest) = trimmed.strip_prefix("- name:") {
            let key = rest.trim().trim_matches('"').trim_matches('\'');
            let env_name = EnvName::new(key);
            if env_name.is_valid_identifier() {
                let col = (line.find(key).unwrap_or(0) + 1) as u32;
                references.push(Reference::new(
                    env_name,
                    path.clone(),
                    Span::point(line_num, col, 0),
                    EvidenceKind::DirectMember,
                ));
            }
        }

        // 2. Docker compose item: - KEY=val or - KEY
        if let Some(rest) = trimmed.strip_prefix("- ") {
            let key = if let Some((k, _)) = rest.split_once('=') {
                k.trim()
            } else {
                rest.trim()
            };
            if key
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                && key.chars().any(|c| c.is_ascii_uppercase())
            {
                let env_name = EnvName::new(key);
                if env_name.is_valid_identifier() {
                    let col = (line.find(key).unwrap_or(0) + 1) as u32;
                    let span = Span::point(line_num, col, 0);
                    references.push(Reference::new(
                        env_name,
                        path.clone(),
                        span,
                        EvidenceKind::DirectMember,
                    ));
                }
            }
        }

        // 3. GitHub Actions expression: ${{ env.VAR }} or ${{ secrets.VAR }}
        for pattern in ["${{ env.", "${{ secrets.", "${env.", "${secrets."] {
            let mut search_from = 0;
            while let Some(pos) = line[search_from..].find(pattern) {
                let abs_pos = search_from + pos;
                let after = &line[abs_pos + pattern.len()..];
                if let Some(end_pos) = after.find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                {
                    let key = &after[..end_pos];
                    let env_name = EnvName::new(key);
                    if env_name.is_valid_identifier() {
                        let col = (abs_pos + 1) as u32;
                        references.push(Reference::new(
                            env_name,
                            path.clone(),
                            Span::point(line_num, col, 0),
                            EvidenceKind::DirectMember,
                        ));
                    }
                }
                search_from = abs_pos + pattern.len();
            }
        }

        // 4. Docker compose variable interpolation: ${VAR} or ${VAR:-default}
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                let start_col = (i + 1) as u32;
                i += 2;
                let var_start = i;
                while i < bytes.len() && bytes[i] != b'}' && bytes[i] != b':' && bytes[i] != b'-' {
                    i += 1;
                }
                if var_start < i {
                    let key = &line[var_start..i];
                    let env_name = EnvName::new(key);
                    if env_name.is_valid_identifier() {
                        references.push(Reference::new(
                            env_name,
                            path.clone(),
                            Span::point(line_num, start_col, 0),
                            EvidenceKind::DirectMember,
                        ));
                    }
                }
            } else {
                i += 1;
            }
        }
    }

    ScanResult {
        references,
        diagnostics: Vec::new(),
    }
}
