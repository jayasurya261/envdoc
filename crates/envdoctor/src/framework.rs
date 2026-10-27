#![allow(clippy::disallowed_methods)]

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Supported framework presets with pre-configured client prefixes and conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FrameworkPreset {
    #[default]
    Auto,
    NextJs,
    Vite,
    Nuxt,
    Astro,
    Rails,
    Django,
    None,
}

impl FrameworkPreset {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "next" | "nextjs" | "next.js" => Some(Self::NextJs),
            "vite" => Some(Self::Vite),
            "nuxt" | "nuxtjs" => Some(Self::Nuxt),
            "astro" => Some(Self::Astro),
            "rails" | "ruby-on-rails" => Some(Self::Rails),
            "django" => Some(Self::Django),
            "none" | "generic" => Some(Self::None),
            _ => None,
        }
    }

    /// Auto-detects framework based on marker files in the workspace directory.
    pub fn detect(root: &Path) -> Self {
        // Next.js
        if root.join("next.config.js").exists()
            || root.join("next.config.mjs").exists()
            || root.join("next.config.ts").exists()
            || root.join("next.config.cjs").exists()
        {
            return Self::NextJs;
        }

        // Vite
        if root.join("vite.config.js").exists()
            || root.join("vite.config.ts").exists()
            || root.join("vite.config.mjs").exists()
            || root.join("vite.config.cjs").exists()
        {
            return Self::Vite;
        }

        // Nuxt
        if root.join("nuxt.config.js").exists() || root.join("nuxt.config.ts").exists() {
            return Self::Nuxt;
        }

        // Astro
        if root.join("astro.config.mjs").exists()
            || root.join("astro.config.ts").exists()
            || root.join("astro.config.js").exists()
        {
            return Self::Astro;
        }

        // Rails
        if root.join("config").join("application.rb").exists() {
            return Self::Rails;
        }
        if let Ok(content) = std::fs::read_to_string(root.join("Gemfile")) {
            if content.contains("'rails'") || content.contains("\"rails\"") {
                return Self::Rails;
            }
        }

        // Django
        if root.join("manage.py").exists() {
            return Self::Django;
        }

        Self::None
    }

    /// Returns the client-exposed environment variable prefixes for this framework.
    pub fn client_prefixes(&self) -> &'static [&'static str] {
        match self {
            Self::NextJs => &["NEXT_PUBLIC_"],
            Self::Vite => &["VITE_"],
            Self::Nuxt => &["NUXT_PUBLIC_"],
            Self::Astro => &["PUBLIC_"],
            _ => &[],
        }
    }

    /// Returns framework-specific platform environment variables to ignore.
    pub fn platform_ignores(&self) -> &'static [&'static str] {
        match self {
            Self::NextJs => &["NEXT_RUNTIME", "PORT", "HOSTNAME", "VERCEL"],
            Self::Vite => &["BASE_URL", "MODE", "DEV", "PROD", "SSR"],
            Self::Nuxt => &["NITRO_PORT", "NITRO_HOST"],
            Self::Astro => &["SITE", "BASE_URL"],
            Self::Rails => &[
                "RAILS_ENV",
                "RAILS_MAX_THREADS",
                "DATABASE_URL",
                "SECRET_KEY_BASE",
            ],
            Self::Django => &["DJANGO_SETTINGS_MODULE", "PYTHONPATH"],
            _ => &[],
        }
    }
}
