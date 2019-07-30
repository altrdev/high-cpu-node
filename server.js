const express = require('express');
const bodyParser = require('body-parser');
const cors = require('cors');
const R = require('ramda');

const config = {
    name: 'high-cpu-node',
    port: 3000,
    host: '0.0.0.0',
};

const app = express();

let shouldRun = true;

app.use(bodyParser.json());
app.use(cors());

app.get('/', (req, res) => {
    res.status(200).send('Status: OK');
});

app.get('/stress', (req, res) => {
    stress(req.query.seconds);
    res.status(200).send("I'm still alive");
});

app.listen(config.port, config.host, (e)=> {
    if(e) {
        console.error(`Internal Server Error: ${e}`);
        throw new Error('Internal Server Error');
    }
    console.log(`${config.name} running on ${config.host}:${config.port}`);
});

function stress(seconds) {
    shouldRun = true;
    const ms = R.isNil(seconds) ? 1000 : seconds * 1000;
    console.log(`Starting stress cpu for ${seconds} seconds`);
    blockCpuFor(ms);
}

function blockCpuFor(ms) {
    let now = new Date().getTime();
    let result = 0;
    while(shouldRun) {
        result += Math.random() * Math.random();
        if (new Date().getTime() > now +ms)
            return;
    }
}
