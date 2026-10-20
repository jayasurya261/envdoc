const dbUrl = process.env.DATABASE_URL;
export const db = { connect: () => dbUrl };
