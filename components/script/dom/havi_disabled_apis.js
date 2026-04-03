// HAVI: lazy bootstrap for location compatibility and disabled APIs.
// Evaluated before parser starts; installs heavy location compatibility only on first use.
(function() {
    var nativeWindowLocationDesc = null;
    var nativeDocumentLocationDesc = null;
    var nativeLocation = null;

    try { nativeWindowLocationDesc = Object.getOwnPropertyDescriptor(window, 'location'); } catch (e) {}
    try { nativeDocumentLocationDesc = Object.getOwnPropertyDescriptor(document, 'location'); } catch (e) {}
    try {
        if (nativeWindowLocationDesc && typeof nativeWindowLocationDesc.get === 'function') {
            nativeLocation = nativeWindowLocationDesc.get.call(window);
        }
    } catch (e) {}
    if (!nativeLocation) {
        try { nativeLocation = window.location; } catch (e) {}
    }

    window.__haviNativeWindowLocationDesc = nativeWindowLocationDesc;
    window.__haviNativeDocumentLocationDesc = nativeDocumentLocationDesc;
    window.__haviNativeLocation = nativeLocation;
    window.__haviNativeHistoryReplaceState = window.history && typeof window.history.replaceState === 'function'
        ? window.history.replaceState.bind(window.history)
        : null;
    window.__haviNativeHistoryPushState = window.history && typeof window.history.pushState === 'function'
        ? window.history.pushState.bind(window.history)
        : null;

    window.__haviCurrentExactHref = typeof window.__haviCurrentExactHref === 'string'
        ? window.__haviCurrentExactHref
        : String(window.__haviExactInitialUrl || '');

    function currentExactHref() {
        try {
            if (nativeLocation && typeof nativeLocation.href === 'string' && nativeLocation.href) {
                return String(nativeLocation.href);
            }
        } catch (e) {}
        try {
            if (nativeWindowLocationDesc && typeof nativeWindowLocationDesc.get === 'function') {
                var loc = nativeWindowLocationDesc.get.call(window);
                if (loc && typeof loc.href === 'string' && loc.href) {
                    return String(loc.href);
                }
            }
        } catch (e2) {}
        if (typeof window.__haviCurrentExactHref === 'string' && window.__haviCurrentExactHref) {
            return String(window.__haviCurrentExactHref);
        }
        return String(window.__haviExactInitialUrl || '');
    }

    function currentScheme() {
        var href = currentExactHref();
        var colon = href.indexOf(':');
        return colon >= 0 ? href.slice(0, colon).toLowerCase() : '';
    }

    function splitJsonqa(href) {
        var literal = href.indexOf('{');
        var encoded = href.search(/%7B/i);
        var idx = literal >= 0 ? literal : encoded;
        return idx >= 0 ? [href.slice(0, idx), href.slice(idx)] : [href, ''];
    }

    function cloneQa(value) {
        var key;
        var i;
        if (value === null || value === undefined) {
            return value;
        }
        if (Array.isArray(value)) {
            return value.map(cloneQa);
        }
        if (typeof value === 'object') {
            var out = {};
            for (key in value) {
                if (Object.prototype.hasOwnProperty.call(value, key)) {
                    out[key] = cloneQa(value[key]);
                }
            }
            return out;
        }
        return value;
    }

    window.__haviSplitJsonqa = splitJsonqa;
    window.__haviCloneQa = cloneQa;

    function fileQaRecord(jsonqa) {
        var dummy = '//file/local/root';
        if (!jsonqa) {
            return { qa: {}, fragment: null, suffix: '' };
        }
        var urc = new URC(dummy + decodeURIComponent(String(jsonqa)));
        return {
            qa: cloneQa(urc.qa) || {},
            fragment: urc.fragment == null ? null : String(urc.fragment),
            suffix: String(urc.href).slice(dummy.length)
        };
    }

    function fileQaSuffix(qa) {
        var dummy = '//file/local/root';
        var keys = Object.keys(qa || {});
        if (keys.length === 0) {
            return '';
        }
        if (keys.length === 1 && keys[0] === '#') {
            return '{#:' + String(qa['#']) + '}';
        }
        var urc = new URC(dummy);
        urc.qa = qa;
        return String(urc.href).slice(dummy.length);
    }

    function fileQaFromSearchAndHash(search, hash) {
        var qa = {};
        var params = new URLSearchParams(String(search || '').replace(/^\?/, ''));
        params.forEach(function(value, key) {
            if (!Object.prototype.hasOwnProperty.call(qa, key)) {
                qa[key] = value;
            } else if (Array.isArray(qa[key])) {
                qa[key].push(value);
            } else {
                qa[key] = [qa[key], value];
            }
        });
        hash = String(hash || '');
        if (hash && hash !== '#') {
            qa['#'] = hash.charAt(0) === '#' ? hash.slice(1) : hash;
        }
        return qa;
    }

    function fileAddressStateFromHref(href) {
        var parts = splitJsonqa(String(href || ''));
        var exact = parts[0];
        var jsonqa = parts[1];
        var qaRecord = fileQaRecord(jsonqa);
        var parsed = new URL(exact);
        return {
            scheme: 'file',
            href: exact + qaRecord.suffix,
            pathname: parsed.pathname || '/',
            qa: qaRecord.qa,
            fragment: qaRecord.fragment,
            isListing: /\/$/.test(parsed.pathname || '/')
        };
    }

    function normalizeFileAddressHref(input, baseHref) {
        var text = String(input);
        var parts = splitJsonqa(text);
        var explicitJsonqa = parts[1] !== '';
        var baseExact = splitJsonqa(String(baseHref || currentExactHref()))[0] || currentExactHref();
        var resolved = new URL(parts[0] || '', baseExact || 'file:///');
        if (resolved.protocol !== 'file:') {
            return resolved.href;
        }
        if (explicitJsonqa) {
            if (resolved.search || resolved.hash) {
                throw new TypeError('Ambiguous file URL: native ?/# cannot be mixed with HAVI JSONqa');
            }
            return resolved.href + fileQaRecord(parts[1]).suffix;
        }
        var stripped = new URL(resolved.href);
        stripped.search = '';
        stripped.hash = '';
        return stripped.href + fileQaSuffix(fileQaFromSearchAndHash(resolved.search, resolved.hash));
    }

    function projectLegacyDocumentUrl() {
        var href = currentExactHref();
        var packet = document.packet || null;
        var parts = splitJsonqa(href);
        var exact = parts[0];
        if (currentScheme() === 'file') {
            return exact;
        }
        if (packet && packet.group && packet.app && packet.location) {
            return 'hppr://' + packet.group + '/' + packet.app + '/' + packet.location;
        }
        return exact.replace(/\/\|\/(plex\/[^/]+\/[^/]+|seal\/[^/]+\/[^/]+\/[^/]+)$/, '');
    }

    function projectDocumentUrc() {
        var packet = document.packet || null;
        if (!packet || !packet.group || !packet.app || !packet.location || !packet.tai || !packet.hash) {
            return null;
        }
        if (packet.type === 'Seal' && packet.sealBy) {
            return '//' + packet.group + '/' + packet.app + '/' + packet.location + '/|/seal/' + packet.sealBy + '/' + packet.tai + '/' + packet.hash;
        }
        if (packet.type === 'Plex') {
            return '//' + packet.group + '/' + packet.app + '/' + packet.location + '/|/plex/' + packet.tai + '/' + packet.hash;
        }
        return null;
    }

    var documentUrlWarned = false;
    function warnDocumentUrlOnce() {
        if (documentUrlWarned) {
            return;
        }
        documentUrlWarned = true;
        try {
            console.warn('HAVI legacy document.URL projection accessed. Use window.address for exact address state and document.URC for exact HPPR packet identity.');
        } catch (e) {}
    }

    try {
        Object.defineProperty(document, 'URL', {
            get: function() {
                warnDocumentUrlOnce();
                return projectLegacyDocumentUrl();
            },
            configurable: true
        });
    } catch (e) {}

    try {
        Object.defineProperty(document, 'documentURI', {
            get: function() {
                return projectLegacyDocumentUrl();
            },
            configurable: true
        });
    } catch (e) {}

    try {
        Object.defineProperty(document, 'URC', {
            get: function() {
                return projectDocumentUrc();
            },
            configurable: true
        });
    } catch (e) {}

    window.__haviFileQaSuffix = fileQaSuffix;
    window.__haviNormalizeFileAddressHref = normalizeFileAddressHref;

    if (currentScheme() === 'file') {
        var fileAddress = {};
        Object.defineProperties(fileAddress, {
            scheme: { get: function() { return 'file'; }, configurable: true },
            href: {
                get: function() { return fileAddressStateFromHref(currentExactHref()).href; },
                set: function(value) {
                    var nextHref = normalizeFileAddressHref(value, currentExactHref());
                    window.__haviCurrentExactHref = nextHref;
                    if (nativeLocation && typeof nativeLocation.assign === 'function') {
                        nativeLocation.assign(nextHref);
                    }
                },
                configurable: true
            },
            pathname: {
                get: function() { return fileAddressStateFromHref(currentExactHref()).pathname; },
                set: function(value) {
                    var state = fileAddressStateFromHref(currentExactHref());
                    this.href = 'file://' + String(value || '/') + fileQaSuffix(state.qa);
                },
                configurable: true
            },
            qa: {
                get: function() { return window.__haviCloneQa(fileAddressStateFromHref(currentExactHref()).qa); },
                set: function(value) {
                    var state = fileAddressStateFromHref(currentExactHref());
                    this.href = splitJsonqa(state.href)[0] + fileQaSuffix(value || {});
                },
                configurable: true
            },
            fragment: {
                get: function() { return fileAddressStateFromHref(currentExactHref()).fragment; },
                configurable: true
            },
            isListing: { get: function() { return fileAddressStateFromHref(currentExactHref()).isListing; }, configurable: true }
        });
        fileAddress.toString = function() { return this.href; };
        try {
            Object.defineProperty(window, 'address', {
                get: function() { return fileAddress; },
                set: function(value) { fileAddress.href = value; },
                configurable: true
            });
        } catch (e) {}
    }

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
            Object.defineProperty(window, net[i], disabled(net[i] + ' is disabled in HAVI. Use window.home or window.route'));
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
