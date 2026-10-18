const fs = require('fs');
const path = require('path');

// The control panel: one static page that reads /info and /metrics and calls the other
// endpoints. Only mounted when UI=1, since it makes /crash a button.
module.exports = function createUi() {
    const html = fs.readFileSync(path.join(__dirname, '..', 'ui.html'));
    return (req, res) => res.type('html').send(html);
};
