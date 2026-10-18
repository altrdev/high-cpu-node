// Reads integer query parameters. `specs` maps a name to {fallback, min, max}: a value that
// is not a number becomes the fallback, one outside min..max is answered with a 400 and
// null is returned, so the caller only has to check for that.
function readParams(req, res, specs) {
    const values = {};
    for (const [name, {fallback, min, max}] of Object.entries(specs)) {
        const parsed = parseInt(req.query[name]);
        const value = isNaN(parsed) ? fallback : parsed;
        if (value < min || value > max) {
            res.status(400).end(`${name} must be between ${min} and ${max}`);
            return null;
        }
        values[name] = value;
    }
    return values;
}

module.exports = {readParams};
