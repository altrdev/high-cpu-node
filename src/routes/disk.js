const fs = require('fs');
const path = require('path');
const {Budget} = require('../budget');
const {readParams} = require('../params');
const {specs} = require('../endpoints');

const PREFIX = 'torchio-disk-';
const chunk = Buffer.alloc(1024 * 1024, 1);

async function writeFile(file, mb) {
    const handle = await fs.promises.open(file, 'w');
    try {
        for (let i = 0; i < mb; i++) {
            await handle.write(chunk, 0, chunk.length);
        }
        await handle.sync();
    } finally {
        await handle.close();
    }
}

// Writes `mb` megabytes to a file in DISK_DIR and keeps it for `hold` seconds. As for
// /memory, MAX_DISK_MB is the total that can be held at the same time.
module.exports = function createDisk(config) {
    const budget = new Budget(config.maxDiskMb);
    const files = new Set();
    let nextId = 0;

    try {
        fs.mkdirSync(config.diskDir, {recursive: true});
        // remove files left behind by a previous run that didn't shut down cleanly
        for (const name of fs.readdirSync(config.diskDir)) {
            if (name.startsWith(PREFIX)) fs.rmSync(path.join(config.diskDir, name), {force: true});
        }
    } catch (e) {
        console.error(`DISK_DIR ${config.diskDir} is not usable, /disk will answer 507: ${e.code || e.message}`);
    }

    const handler = async (req, res) => {
        const params = readParams(req, res, specs(config).disk);
        if (!params) return;
        const {mb, hold} = params;

        const releaseBudget = budget.reserve(mb);
        if (!releaseBudget) {
            return res.status(429).end(`${budget.held} MB already held, limit is ${budget.limit} MB`);
        }

        const file = path.join(config.diskDir, `${PREFIX}${nextId++}`);
        files.add(file);
        const release = async () => {
            files.delete(file);
            releaseBudget();
            await fs.promises.rm(file, {force: true}).catch(() => {});
        };

        try {
            await writeFile(file, mb);
        } catch (e) {
            await release();
            return res.status(507).end(`Could not write ${mb} MB to ${config.diskDir}: ${e.code || e.message}`);
        }
        setTimeout(release, hold * 1000);
        res.status(200).end(`Wrote ${mb} MB to ${config.diskDir}, held for ${hold} s`);
    };

    // Used on shutdown, which can't wait for the timers
    const removeFiles = () => files.forEach((file) => fs.rmSync(file, {force: true}));

    return {handler, budget, removeFiles};
};
