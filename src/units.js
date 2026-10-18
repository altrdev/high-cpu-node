const MB = 1024 * 1024;
const toMb = (bytes) => Math.round(bytes / MB);

module.exports = {MB, toMb};
