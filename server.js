const config = require('./src/config');
const {createApp} = require('./src/app');

const {app, stop} = createApp(config);

const server = app.listen(config.port, config.host, () => {
    console.log(`${config.name} running on ${config.host}:${config.port} with ${config.workers} workers`);
});

server.on('error', (e) => {
    console.error(`Internal Server Error: ${e}`);
    process.exit(1);
});

function shutdown() {
    server.close();
    stop();
    process.exit(0);
}

process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);
