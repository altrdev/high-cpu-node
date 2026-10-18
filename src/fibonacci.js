process.on("message", ({id, num}) => {
    process.send({id, value: fibo(num - 1)});
});

function fibo(n) {
    if (n < 2)
        return 1;
    else
        return fibo(n - 2) + fibo(n - 1);
}
