// HAVI: lazy bootstrap for location compatibility and disabled APIs.
// Evaluated before parser starts; installs heavy location compatibility only on first use.
(function() {
    var nativeWindowLocationDesc = null;
    var nativeLocation = null;

    try { nativeWindowLocationDesc = Object.getOwnPropertyDescriptor(window, 'location'); } catch (e) {}
    try {
        if (nativeWindowLocationDesc && typeof nativeWindowLocationDesc.get === 'function') {
            nativeLocation = nativeWindowLocationDesc.get.call(window);
        }
    } catch (e) {}
    if (!nativeLocation) {
        try { nativeLocation = window.location; } catch (e) {}
    }

    window.__haviNativeWindowLocationDesc = nativeWindowLocationDesc;
    window.__haviNativeLocation = nativeLocation;
    window.__haviNativeHistoryReplaceState = window.history && typeof window.history.replaceState === 'function'
        ? window.history.replaceState.bind(window.history)
        : null;
    window.__haviNativeHistoryPushState = window.history && typeof window.history.pushState === 'function'
        ? window.history.pushState.bind(window.history)
        : null;

    function disabled(msg) {
        return {
            get: function() { throw new TypeError(msg); },
            set: function() { throw new TypeError(msg); },
            configurable: true
        };
    }

    function ensureLocationCompat() {
        if (window.__haviLocationCompatReady) {
            return true;
        }
        if (window.__haviLocationCompatLoading) {
            return false;
        }
        window.__haviLocationCompatLoading = true;
        try {
            if (typeof window.__haviLocationCompatSource !== 'string') {
                throw new TypeError('HAVI location compatibility bootstrap is unavailable');
            }
            (0, eval)(window.__haviLocationCompatSource);
            window.__haviLocationCompatSource = null;
        } finally {
            window.__haviLocationCompatLoading = false;
        }
        return !!window.__haviLocationCompatReady;
    }

    function withLocationCompat(action) {
        if (!ensureLocationCompat()) {
            throw new TypeError('HAVI location compatibility failed to initialize');
        }
        return action();
    }

    try {
        Object.defineProperty(window, 'location', {
            get: function() {
                return withLocationCompat(function() {
                    return typeof window.__haviEnsureCompatLocation === 'function'
                        ? window.__haviEnsureCompatLocation()
                        : window.__haviCompatLocation;
                });
            },
            set: function(value) {
                return withLocationCompat(function() {
                    var location = typeof window.__haviEnsureCompatLocation === 'function'
                        ? window.__haviEnsureCompatLocation()
                        : window.__haviCompatLocation;
                    location.href = value;
                });
            },
            configurable: true
        });
    } catch (e) {}

    try {
        Object.defineProperty(document, 'location', {
            get: function() {
                return withLocationCompat(function() {
                    return typeof window.__haviEnsureCompatLocation === 'function'
                        ? window.__haviEnsureCompatLocation()
                        : window.__haviCompatLocation;
                });
            },
            set: function(value) {
                return withLocationCompat(function() {
                    var location = typeof window.__haviEnsureCompatLocation === 'function'
                        ? window.__haviEnsureCompatLocation()
                        : window.__haviCompatLocation;
                    location.href = value;
                });
            },
            configurable: true
        });
    } catch (e) {}

    if (window.history) {
        try {
            Object.defineProperty(window.history, 'replaceState', {
                value: function() {
                    var args = arguments;
                    return withLocationCompat(function() {
                        return window.__haviCompatHistoryReplaceState.apply(window.history, args);
                    });
                },
                configurable: true,
                writable: true
            });
        } catch (e) {}

        try {
            Object.defineProperty(window.history, 'pushState', {
                value: function() {
                    var args = arguments;
                    return withLocationCompat(function() {
                        return window.__haviCompatHistoryPushState.apply(window.history, args);
                    });
                },
                configurable: true,
                writable: true
            });
        } catch (e) {}
    }

    var net = ['WebSocket', 'XMLHttpRequest', 'EventSource'];
    for (var i = 0; i < net.length; i++) {
        try {
            Object.defineProperty(window, net[i], disabled(net[i] + ' is disabled in HAVI. Use window.source.client when available'));
        } catch (e) {}
    }

    var doc = ['write', 'writeln', 'open', 'close'];
    for (var j = 0; j < doc.length; j++) {
        (function(name) {
            try {
                Object.defineProperty(document, name, {
                    value: function() {
                        throw new TypeError('document.' + name + '() is disabled in HAVI');
                    },
                    writable: false,
                    configurable: true
                });
            } catch (e) {}
        })(doc[j]);
    }

    window.__haviCompatLocation = null;
    window.__haviCompatHistoryReplaceState = null;
    window.__haviCompatHistoryPushState = null;
    window.__haviLocationCompatReady = false;
    window.__haviLocationCompatLoading = false;
})();
