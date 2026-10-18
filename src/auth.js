const crypto = require('crypto');

const digest = (value) => crypto.createHash('sha256').update(value).digest();

// With TOKEN set, the endpoints that change the state of the instance need
// `Authorization: Bearer <token>`. Without it nothing is checked.
module.exports = function createAuth(config) {
    if (!config.token) return (req, res, next) => next();
    const expected = digest(config.token);
    return (req, res, next) => {
        const [scheme, value = ''] = (req.get('authorization') || '').split(' ');
        if (scheme.toLowerCase() === 'bearer' && crypto.timingSafeEqual(digest(value), expected)) return next();
        res.set('WWW-Authenticate', 'Bearer').status(401).end('Unauthorized: send the token as "Authorization: Bearer <token>"');
    };
};
