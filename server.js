const express = require('express');
const bodyParser = require('body-parser');
const cors = require('cors');
const R = require('ramda');
const {fork} = require('child_process');
const child = fork(`${__dirname}/fibonacci.js`);
let {EventEmitter} = require('events');

const config = {
    name: 'high-cpu-node',
    port: 3000,
    host: '0.0.0.0',
};

const app = express();
let event = new EventEmitter();

app.use(bodyParser.json());
app.use(cors());

app.get('/', async (req, res) => {
    res.status(200).end('Status: OK');
});

app.get('/stress', async (req, res) => {
    let fiboNumber = parseInt(req.query.number);
    fiboNumber = isNaN(fiboNumber)  ? 45 : fiboNumber;
    stress(fiboNumber, res);
});

child.on("message",(msg)=> event.emit(msg.event,msg.value));

app.listen(config.port, config.host, (e)=> {
    if(e) {
        console.error(`Internal Server Error: ${e}`);
        throw new Error('Internal Server Error');
    }
    console.log(`${config.name} running on ${config.host}:${config.port}`);
});

function stress(number, res) {
    let eventNumber = Math.random() * 100;

    child.send({num:number,event:eventNumber});

    event.once(eventNumber, (value) => {
        res.status(200).end(`I'm still alive after fibonacci calculation with result: ${value}`);
    })
}

