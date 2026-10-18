const http = require('http');
const config = require('./src/config');
const {createApp} = require('./src/app');

const {app, stop} = createApp(config);

const server = http.createServer(app);

server.on('error', (e) => {
    console.error(`Internal Server Error: ${e}`);
    process.exit(1);
});

const listen = () =>
    server.listen(config.port, config.host, () => {
        console.log(`${config.name} running on ${config.host}:${config.port} with ${config.workers} workers`);
    });

// START_DELAY_SECONDS: the port stays closed for a while, like an application that is slow to start
let startTimer = null;
if (config.startDelaySeconds > 0) {
    console.log(`${config.name} will start listening in ${config.startDelaySeconds} s`);
    startTimer = setTimeout(listen, config.startDelaySeconds * 1000);
} else {
    listen();
}

function shutdown() {
    clearTimeout(startTimer);
    server.close();
    stop();
    process.exit(0);
}

process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);
