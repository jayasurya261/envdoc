const port = process.env.STATIC_PORT;
const dynKey = "COMPUTED_KEY";
const val = process.env[dynKey];
console.log(port, val);

