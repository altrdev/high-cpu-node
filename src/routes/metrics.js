const {MB} = require('../units');

const escapeLabel = (value) => String(value).replace(/\\/g, '\\\\').replace(/\n/g, '\\n').replace(/"/g, '\\"');

// Adds a metric to `lines`; one without samples is left out altogether
function metric(lines, name, type, help, samples) {
    if (samples.length === 0) return;
    lines.push(`# HELP ${name} ${help}`, `# TYPE ${name} ${type}`);
    for (const [labels, value] of samples) {
        const text = labels ? `{${Object.entries(labels).map(([k, v]) => `${k}="${escapeLabel(v)}"`).join(',')}}` : '';
        lines.push(`${name}${text} ${value}`);
    }
}

// The figures of /info in the Prometheus text format
module.exports = function createMetrics({version, startedAt, snapshot, counters}) {
    return async (req, res) => {
        const {stress, delayInFlight, memory, disk, fd, ready} = await snapshot();
        const lines = [];

        metric(lines, 'torchio_info', 'gauge', 'Version of the server.', [[{version}, 1]]);
        metric(lines, 'torchio_start_time_seconds', 'gauge', 'Start time of the server, in Unix time.', [[null, startedAt / 1000]]);
        metric(lines, 'torchio_workers', 'gauge', 'Worker processes for /stress.', [[null, stress.workers]]);
        metric(lines, 'torchio_stress_requests_in_flight', 'gauge', '/stress requests being calculated.', [[null, stress.inFlight]]);
        metric(lines, 'torchio_worker_requests_in_flight', 'gauge', '/stress requests being calculated, by worker.',
            stress.perWorker.map((count, index) => [{worker: index}, count]));
        metric(lines, 'torchio_cpu_burn_percent', 'gauge', 'Share of the time each worker is kept busy by /cpu, 0 when it is not running.', [[null, stress.burn ? stress.burn.percent : 0]]);
        metric(lines, 'torchio_delay_requests_in_flight', 'gauge', '/delay requests still waiting.', [[null, delayInFlight]]);
        metric(lines, 'torchio_memory_held_bytes', 'gauge', 'Memory held by /memory.', [[null, memory.heldMb * MB]]);
        metric(lines, 'torchio_memory_limit_bytes', 'gauge', 'Total memory /memory may hold (MAX_MEMORY_MB).', [[null, memory.limitMb * MB]]);
        metric(lines, 'process_resident_memory_bytes', 'gauge', 'Resident memory of the server process.', [[null, memory.rssBytes]]);
        metric(lines, 'torchio_container_memory_limit_bytes', 'gauge', 'Memory limit of the container, when there is one.',
            memory.containerLimitBytes === null ? [] : [[null, memory.containerLimitBytes]]);
        metric(lines, 'torchio_disk_held_bytes', 'gauge', 'Disk space held by /disk.', [[null, disk.heldMb * MB]]);
        metric(lines, 'torchio_disk_limit_bytes', 'gauge', 'Total disk space /disk may hold (MAX_DISK_MB).', [[null, disk.limitMb * MB]]);
        metric(lines, 'torchio_disk_free_bytes', 'gauge', 'Free space in DISK_DIR.', disk.freeBytes === null ? [] : [[null, disk.freeBytes]]);
        metric(lines, 'torchio_fd_held', 'gauge', 'File descriptors held by /fd.', [[null, fd.held]]);
        metric(lines, 'torchio_fd_limit', 'gauge', 'Total file descriptors /fd may hold (MAX_FD).', [[null, fd.limit]]);
        metric(lines, 'torchio_ready', 'gauge', '1 when /ready answers 200, 0 while it answers 503.', [[null, ready.ready ? 1 : 0]]);
        metric(lines, 'torchio_http_requests_total', 'counter', 'Requests answered, by route and status code.',
            counters.entries().map(({route, code, count}) => [{route, code}, count]));

        res.type('text/plain; version=0.0.4; charset=utf-8').send(lines.join('\n') + '\n');
    };
};
