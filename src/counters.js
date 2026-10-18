// Requests answered, by route and status code, for /metrics. The route is the pattern
// (/status/:code), not the URL, so the number of series stays small.
function createRequestCounter() {
    const counts = new Map(); // `${route}\0${code}` -> count

    const middleware = (req, res, next) => {
        res.on('finish', () => {
            const key = `${req.route ? req.route.path : 'other'}\u0000${res.statusCode}`;
            counts.set(key, (counts.get(key) || 0) + 1);
        });
        next();
    };

    const entries = () =>
        [...counts].map(([key, count]) => {
            const [route, code] = key.split('\u0000');
            return {route, code, count};
        });

    return {middleware, entries};
}

module.exports = {createRequestCounter};
