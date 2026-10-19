const {readParams} = require('../params');
const {specs} = require('../endpoints');

// A readiness switch you can flip over HTTP: /ready answers 200 or 503, /ready/off makes it
// answer 503 (for `seconds`, or until /ready/on when 0) so you can watch what a readiness
// probe or a load balancer does with an instance that leaves the rotation but is still alive.
module.exports = function createReady(config) {
    let down = false;
    let until = null; // when it becomes ready again, null if only /ready/on does it
    let timer = null;

    const turnOn = () => {
        clearTimeout(timer);
        down = false;
        until = null;
    };

    const state = () => ({
        ready: !down,
        secondsLeft: down && until !== null ? Math.max(0, Math.ceil((until - Date.now()) / 1000)) : null,
    });

    const handler = (req, res) => {
        const {ready, secondsLeft} = state();
        if (ready) return res.status(200).end('Ready');
        res.status(503).end(secondsLeft === null ? 'Not ready' : `Not ready, ready again in ${secondsLeft} s`);
    };

    const off = (req, res) => {
        const params = readParams(req, res, specs(config).readyOff);
        if (!params) return;
        const {seconds} = params;

        turnOn();
        down = true;
        if (seconds > 0) {
            until = Date.now() + seconds * 1000;
            timer = setTimeout(turnOn, seconds * 1000);
        }
        res.status(200).end(seconds > 0 ? `Not ready for ${seconds} s` : 'Not ready until /ready/on');
    };

    const on = (req, res) => {
        turnOn();
        res.status(200).end('Ready');
    };

    return {handler, off, on, state, stop: turnOn};
};
