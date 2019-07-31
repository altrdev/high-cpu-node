process.on("message", (msg) => {
    "use strict";
    process.send({value: fibo(msg.num-1),event:msg.event})
});

function fibo(n) {
    if (n < 2)
        return 1;
    else
        return fibo(n - 2) + fibo(n - 1);
}
