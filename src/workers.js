const {fork} = require('child_process');
const path = require('path');

// One child process per core: the calculation of /stress would block the event loop,
// and requests are dispatched round-robin so concurrent ones can use several cores.
class WorkerPool {
    constructor(count) {
        this.pending = new Map(); // request id -> {res, worker}
        this.workers = [];
        this.next = 0;
        this.nextId = 0;
        this.stopping = false;
        this.burning = null; // {percent, endsAt} of the last /cpu
        for (let i = 0; i < count; i++) {
            this.workers.push(this.spawn(i));
        }
    }

    spawn(index) {
        const worker = fork(path.join(__dirname, 'fibonacci.js'));

        this.sendBurn(worker);

        worker.on('message', ({id, value}) => {
            const request = this.pending.get(id);
            if (!request) return; // client already disconnected
            this.pending.delete(id);
            request.res.status(200).end(`I'm still alive after fibonacci calculation with result: ${value}`);
        });

        worker.on('exit', (code, signal) => {
            if (this.stopping) return;
            console.error(`worker ${index} exited (code: ${code}, signal: ${signal}), restarting`);
            for (const [id, request] of this.pending) {
                if (request.worker === worker) {
                    this.pending.delete(id);
                    request.res.status(500).end('Worker crashed');
                }
            }
            this.workers[index] = this.spawn(index);
        });

        return worker;
    }

    sendBurn(worker) {
        if (this.burning && worker.connected) worker.send({burn: this.burning});
    }

    // Keeps every worker busy for `percent` of the time during `seconds`, 0 to stop
    burn(percent, seconds) {
        this.burning = {percent, endsAt: Date.now() + seconds * 1000};
        this.workers.forEach((worker) => this.sendBurn(worker));
    }

    run(number, res) {
        const id = this.nextId++;
        const worker = this.workers[this.next];
        this.next = (this.next + 1) % this.workers.length;

        this.pending.set(id, {res, worker});
        res.on('close', () => this.pending.delete(id));
        worker.send({id, num: number});
    }

    snapshot() {
        const perWorker = this.workers.map(() => 0);
        for (const request of this.pending.values()) {
            perWorker[this.workers.indexOf(request.worker)]++;
        }
        const secondsLeft = this.burning ? Math.ceil((this.burning.endsAt - Date.now()) / 1000) : 0;
        const burn = secondsLeft > 0 && this.burning.percent > 0 ? {percent: this.burning.percent, secondsLeft} : null;
        return {workers: this.workers.length, inFlight: this.pending.size, perWorker, burn};
    }

    stop() {
        this.stopping = true;
        this.workers.forEach((worker) => worker.kill());
    }
}

module.exports = {WorkerPool};
