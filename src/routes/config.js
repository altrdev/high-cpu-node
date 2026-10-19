// The effective configuration, as the server reads it from the environment. The token is never
// shown, only whether there is one.
module.exports = function createConfig({config}) {
    return (req, res) => {
        const {token, ...rest} = config;
        res.json({...rest, auth: Boolean(token)});
    };
};
