// A total that concurrent requests share: what is held right now against a limit,
// so they can't grow it unbounded.
class Budget {
    constructor(limit) {
        this.limit = limit;
        this.held = 0;
    }

    // Reserves `amount`. Returns the function that gives it back, or null when it doesn't fit.
    reserve(amount) {
        if (this.held + amount > this.limit) return null;
        this.held += amount;
        let released = false;
        return () => {
            if (released) return;
            released = true;
            this.held -= amount;
        };
    }
}

module.exports = {Budget};
