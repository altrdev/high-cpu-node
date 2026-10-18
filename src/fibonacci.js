// A worker answers /stress requests and, when asked, keeps its core busy for a share of the time
const PERIOD_MS = 100;
let burnTimer = null;

process.on("message", ({id, num, burn}) => {
    if (burn) return startBurn(burn);
    process.send({id, value: fibo(num - 1)});
});

// Spins for `percent` of every period and sleeps for the rest, until `endsAt`. The sleeps
// leave the event loop free, so a /stress request is still picked up in the meantime.
function startBurn({percent, endsAt}) {
    clearTimeout(burnTimer);
    const slice = () => {
        const start = Date.now();
        if (start >= endsAt) return;
        const busyUntil = start + (PERIOD_MS * percent) / 100;
        while (Date.now() < busyUntil);
        burnTimer = setTimeout(slice, Math.max(0, PERIOD_MS - (Date.now() - start)));
    };
    slice();
}

function fibo(n) {
    if (n < 2)
        return 1;
    else
        return fibo(n - 2) + fibo(n - 1);
}
