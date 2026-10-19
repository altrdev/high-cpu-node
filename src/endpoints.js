// The definition of every endpoint: what it does, whether TOKEN protects it and the parameters
// it takes with their defaults and limits. The routes read their parameters from `specs` and
// /help is built from `catalog`, so the two can't go out of date.
const STATUS_CODES = {min: 200, max: 599};

function specs(config) {
    const hold = {fallback: config.defaultHoldSeconds, min: 0, max: config.maxHoldSeconds};
    return {
        stress: {number: {fallback: config.defaultNumber, min: 0, max: config.maxNumber}},
        cpu: {percent: {fallback: config.defaultCpuPercent, min: 0, max: 100}, hold},
        delay: {ms: {fallback: config.defaultDelayMs, min: 0, max: config.maxDelayMs}},
        memory: {mb: {fallback: config.defaultMemoryMb, min: 1, max: config.maxMemoryMb}, hold},
        leak: {
            rate: {fallback: config.defaultLeakRateMb, min: 1, max: config.maxMemoryMb},
            mb: {fallback: config.defaultMemoryMb, min: 1, max: config.maxMemoryMb},
            hold,
        },
        disk: {mb: {fallback: config.defaultDiskMb, min: 1, max: config.maxDiskMb}, hold},
        fd: {count: {fallback: config.defaultFd, min: 1, max: config.maxFd}, hold},
        readyOff: {seconds: {fallback: config.defaultNotReadySeconds, min: 0, max: config.maxHoldSeconds}},
        log: {
            lines: {fallback: config.defaultLogLines, min: 1, max: config.maxLogLines},
            rate: {fallback: config.defaultLogRate, min: 1, max: config.maxLogRate},
        },
        crash: {code: {fallback: 1, min: 0, max: 255}, after: {fallback: 0, min: 0, max: config.maxHoldSeconds}},
    };
}

function catalog(config) {
    const s = specs(config);
    const list = [
        {path: '/', auth: false, description: 'Health check, returns "Status: OK"'},
        {path: '/stress', auth: true, params: s.stress, description: 'Runs a Fibonacci calculation in a worker, to keep a core busy'},
        {path: '/cpu', auth: true, params: s.cpu, description: 'Keeps the workers busy for a share of the time, percent=0 stops it'},
        {path: '/delay', auth: false, params: s.delay, description: 'Answers after a delay, without blocking the server'},
        {
            path: '/status/:code', auth: false,
            params: {code: {...STATUS_CODES, in: 'path'}},
            description: 'Answers with the given HTTP status',
        },
        {path: '/memory', auth: true, params: s.memory, description: 'Allocates memory (MB) and holds it'},
        {path: '/leak', auth: true, params: s.leak, description: 'Memory that grows by itself, rate MB each second up to mb MB'},
        {path: '/disk', auth: true, params: s.disk, description: 'Writes a file (MB) in DISK_DIR and keeps it'},
        {path: '/fd', auth: true, params: s.fd, description: 'Opens file descriptors and holds them'},
        {path: '/ready', auth: false, description: 'Readiness check: 200, or 503 after /ready/off'},
        {path: '/ready/off', auth: true, params: s.readyOff, description: 'Makes /ready answer 503 for seconds (0 until /ready/on)'},
        {path: '/ready/on', auth: true, description: 'Makes /ready answer 200 again'},
        {path: '/crash', auth: true, params: s.crash, description: 'Ends the process with the exit code, after the given seconds'},
        {path: '/log', auth: true, params: s.log, description: 'Writes lines to stdout at rate lines/s, then stops'},
        {path: '/echo', auth: false, description: 'The request as the server sees it: headers, client address, protocol (credentials masked)'},
        {path: '/info', auth: false, description: 'JSON with what the server is doing and the remaining budgets'},
        {path: '/metrics', auth: false, description: 'The same figures in the Prometheus text format'},
        {path: '/help', auth: false, description: 'This list'},
        {path: '/config', auth: false, description: 'The effective configuration, without the token'},
    ];
    if (config.ui) list.splice(list.findIndex((e) => e.path === '/info'), 0, {path: '/ui', auth: false, description: 'Control panel in the browser'});
    return list;
}

function toText(version, endpoints) {
    const lines = [`torchio ${version}`, ''];
    for (const {path, auth, params = {}, description} of endpoints) {
        lines.push(`${path}${auth ? '  [needs TOKEN]' : ''}`, `  ${description}`);
        for (const [name, {fallback, min, max}] of Object.entries(params)) {
            lines.push(`  ${name}: ${fallback === undefined ? 'required' : `default ${fallback}`}, ${min}..${max}`);
        }
    }
    return lines.join('\n') + '\n';
}

module.exports = {specs, catalog, toText, STATUS_CODES};
