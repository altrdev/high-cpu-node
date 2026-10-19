const {catalog, toText} = require('../endpoints');

// Lists every endpoint with its parameters, defaults and limits: JSON, or plain text for a browser
module.exports = function createHelp({config, version}) {
    return (req, res) => {
        const endpoints = catalog(config);
        if (req.accepts(['json', 'html']) === 'html') {
            return res.type('text/plain').end(toText(version, endpoints));
        }
        res.json({name: config.name, version, endpoints});
    };
};
