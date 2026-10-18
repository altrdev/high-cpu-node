// Answers with the requested HTTP status code, to test probes and retries
module.exports = function status(req, res) {
    const code = Number(req.params.code);
    if (!Number.isInteger(code) || code < 200 || code > 599) {
        return res.status(400).end('code must be an integer between 200 and 599');
    }
    res.status(code).end(`Status: ${code}`);
};
