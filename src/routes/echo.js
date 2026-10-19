// Headers that carry credentials are never echoed back
const MASKED = new Set(['authorization', 'proxy-authorization', 'cookie', 'set-cookie']);

// Returns the request as the server sees it, to show what a proxy, an ingress controller or a
// load balancer adds, removes or rewrites (Host, X-Forwarded-*, the client address).
// `req.ip` follows TRUST_PROXY, so it shows the address a proxy would pass on.
module.exports = function createEcho() {
    return (req, res) => {
        const headers = {};
        for (const [name, value] of Object.entries(req.headers)) headers[name] = MASKED.has(name) ? '[masked]' : value;
        res.json({
            method: req.method,
            url: req.originalUrl,
            protocol: req.protocol,
            secure: req.secure,
            httpVersion: req.httpVersion,
            hostname: req.hostname,
            ip: req.ip,
            ips: req.ips,
            remoteAddress: req.socket.remoteAddress,
            headers,
        });
    };
};
