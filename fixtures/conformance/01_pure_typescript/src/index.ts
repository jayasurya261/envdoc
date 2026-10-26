const port = process.env.PORT;
const jwt = process.env.JWT_SECRET;
if (process.env.DEBUG) {
    console.log(`Server starting on port ${port} with ${jwt}`);
}

