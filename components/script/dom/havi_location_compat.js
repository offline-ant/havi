// HAVI: compatibility shim for legacy web APIs on hppr* and file pages.
// Loaded lazily on first location or history compatibility use.
(function() {
    if (window.__haviLocationCompatReady) {
        return;
    }
    var hasOwn = Object.prototype.hasOwnProperty;
    var nativeWindowLocationDesc = window.__haviNativeWindowLocationDesc || null;
    var nativeLocation = window.__haviNativeLocation || null;
    var nativeHistoryReplaceState = window.__haviNativeHistoryReplaceState || null;
    var nativeHistoryPushState = window.__haviNativeHistoryPushState || null;
    var warned = false;
    var compatLocation = null;
    var compatHistoryHref = null;
    var compatHistoryHash = null;

    if (!nativeWindowLocationDesc) {
        try { nativeWindowLocationDesc = Object.getOwnPropertyDescriptor(window, 'location'); } catch (e) {}
    }
    if (!nativeLocation) {
        try {
            if (nativeWindowLocationDesc && typeof nativeWindowLocationDesc.get === 'function') {
                nativeLocation = nativeWindowLocationDesc.get.call(window);
            }
        } catch (e) {}
    }
    if (!nativeLocation) {
        try { nativeLocation = window.location; } catch (e) {}
    }
    if (!nativeHistoryReplaceState) {
        try {
            if (window.history && typeof window.history.replaceState === 'function') {
                nativeHistoryReplaceState = window.history.replaceState.bind(window.history);
            }
        } catch (e) {}
    }
    if (!nativeHistoryPushState) {
        try {
            if (window.history && typeof window.history.pushState === 'function') {
                nativeHistoryPushState = window.history.pushState.bind(window.history);
            }
        } catch (e) {}
    }

    function warnOnce() {
        if (warned) {
            return;
        }
        warned = true;
        try {
            if (window.console && typeof window.console.warn === 'function') {
                window.console.warn(
                    'HAVI compatibility mode: window.location and document.location are shimmed for legacy libraries. ' +
                    'They expose web-style pathname/search/hash over HAVI address and JSONqa. ' +
                    'Use window.address for exact HAVI semantics. JSONqa input is not accepted through window.location.'
                );
            }
        } catch (e) {}
    }

    function disabled(msg) {
        return {
            get: function() { throw new TypeError(msg); },
            set: function() { throw new TypeError(msg); },
            configurable: true
        };
    }

    function isHpprSchemeName(scheme) {
        return scheme === 'hppr' || scheme.indexOf('hppr-') === 0;
    }

    function currentSchemeName() {
        var href = '';
        try {
            if (window.address && typeof window.address.href === 'string') {
                href = String(window.address.href);
            }
        } catch (e) {}
        if (!href) {
            href = String((window.__haviNativeLocation && window.__haviNativeLocation.href) || '');
        }
        var colon = href.indexOf(':');
        return colon >= 0 ? href.slice(0, colon).toLowerCase() : '';
    }

    function isHpprPage() {
        return isHpprSchemeName(currentSchemeName());
    }

    function isFilePage() {
        return currentSchemeName() === 'file';
    }

    function throwJsonqaInputError() {
        throw new TypeError('window.location compatibility mode does not accept HAVI JSONqa. Use window.address');
    }

    function rejectJsonqaInput(value) {
        if (/[{}]/.test(String(value))) {
            throwJsonqaInputError();
        }
    }

    function splitJsonqa(href) {
        var idx = String(href || '').indexOf('{');
        return idx >= 0 ? [String(href).slice(0, idx), String(href).slice(idx)] : [String(href || ''), ''];
    }

    function cloneQaValue(value) {
        var i;
        var out;
        if (value === null || value === undefined) {
            return value;
        }
        if (Array.isArray(value)) {
            out = new Array(value.length);
            for (i = 0; i < value.length; i++) {
                out[i] = cloneQaValue(value[i]);
            }
            return out;
        }
        if (typeof value === 'object') {
            out = {};
            for (i in value) {
                if (hasOwn.call(value, i)) {
                    out[i] = cloneQaValue(value[i]);
                }
            }
            return out;
        }
        return value;
    }

    function currentHpprState() {
        var addr;
        var qa;
        try {
            addr = window.address;
        } catch (e) {
            return null;
        }
        if (!addr) {
            return null;
        }
        qa = cloneQaValue(addr.qa);
        if (!qa || typeof qa !== 'object' || Array.isArray(qa)) {
            qa = {};
        }
        return {
            scheme: addr.scheme ? String(addr.scheme) : 'hppr',
            group: addr.group == null ? '' : String(addr.group),
            api: addr.api == null ? '' : String(addr.api),
            key: addr.key == null ? '' : String(addr.key),
            isListing: !!addr.isListing,
            qa: qa
        };
    }

    function cloneHpprState(state) {
        return {
            scheme: state.scheme,
            group: state.group,
            api: state.api,
            key: state.key,
            isListing: !!state.isListing,
            qa: cloneQaValue(state.qa) || {}
        };
    }

    function needsQuoting(text) {
        return text === '' || /[\s{}\[\],:\\"']/.test(text);
    }

    function quoteText(text) {
        return '"' + String(text).replace(/\\/g, '\\\\').replace(/"/g, '\\"') + '"';
    }

    function formatQaKey(key) {
        key = String(key);
        return needsQuoting(key) ? quoteText(key) : key;
    }

    function formatQaValue(value, keyName) {
        var i;
        var parts;
        var text;
        if (value === null || value === undefined) {
            value = '';
        }
        if (Array.isArray(value)) {
            parts = [];
            for (i = 0; i < value.length; i++) {
                parts.push(formatQaValue(value[i], keyName));
            }
            return '[' + parts.join(',') + ']';
        }
        if (typeof value === 'object') {
            return formatQaObject(value);
        }
        text = String(value);
        if (keyName === 'via') {
            return text;
        }
        return needsQuoting(text) ? quoteText(text) : text;
    }

    function formatQaObject(obj) {
        var parts = [];
        var key;
        for (key in obj) {
            if (hasOwn.call(obj, key) && obj[key] !== undefined) {
                parts.push(formatQaKey(key) + ':' + formatQaValue(obj[key], key));
            }
        }
        return '{' + parts.join(',') + '}';
    }

    function qaHasEntries(obj) {
        var key;
        for (key in obj) {
            if (hasOwn.call(obj, key) && obj[key] !== undefined) {
                return true;
            }
        }
        return false;
    }

    function serializeQa(obj) {
        if (!obj || typeof obj !== 'object' || !qaHasEntries(obj)) {
            return '';
        }
        return formatQaObject(obj);
    }

    function projectedFragment(qa) {
        if (!qa || !hasOwn.call(qa, '#')) {
            return null;
        }
        if (qa['#'] === null || qa['#'] === undefined) {
            return null;
        }
        return String(qa['#']);
    }

    function projectedHash(qa) {
        var fragment = projectedFragment(qa);
        return fragment === null || fragment === '' ? '' : '#' + fragment;
    }

    function appendProjectedSearch(params, key, value) {
        var i;
        if (value === null || value === undefined) {
            params.append(key, '');
            return;
        }
        if (Array.isArray(value)) {
            for (i = 0; i < value.length; i++) {
                params.append(key, String(value[i]));
            }
            return;
        }
        if (typeof value === 'object') {
            return;
        }
        params.append(key, String(value));
    }

    function projectedSearch(qa) {
        var params = new URLSearchParams();
        var key;
        var text;
        if (qa && typeof qa === 'object') {
            for (key in qa) {
                if (hasOwn.call(qa, key) && key !== '#' && key !== 'via') {
                    appendProjectedSearch(params, key, qa[key]);
                }
            }
        }
        text = params.toString();
        return text ? '?' + text : '';
    }

    function stripProjectedSearch(qa) {
        var key;
        var out = {};
        if (!qa || typeof qa !== 'object') {
            return out;
        }
        for (key in qa) {
            if (!hasOwn.call(qa, key)) {
                continue;
            }
            if (key === '#' || key === 'via') {
                out[key] = cloneQaValue(qa[key]);
                continue;
            }
            if (qa[key] && typeof qa[key] === 'object' && !Array.isArray(qa[key])) {
                out[key] = cloneQaValue(qa[key]);
            }
        }
        return out;
    }

    function searchToProjectedQa(searchText) {
        var params = new URLSearchParams(String(searchText || '').replace(/^\?/, ''));
        var out = {};
        params.forEach(function(value, key) {
            if (!hasOwn.call(out, key)) {
                out[key] = value;
            } else if (Array.isArray(out[key])) {
                out[key].push(value);
            } else {
                out[key] = [out[key], value];
            }
        });
        return out;
    }

    function applyProjectedSearchAndHash(baseQa, searchText, hashText) {
        var key;
        var projected = searchToProjectedQa(searchText);
        var qa = stripProjectedSearch(baseQa);
        for (key in projected) {
            if (hasOwn.call(projected, key)) {
                qa[key] = projected[key];
            }
        }
        hashText = String(hashText || '');
        if (hashText === '' || hashText === '#') {
            delete qa['#'];
        } else {
            qa['#'] = hashText.charAt(0) === '#' ? hashText.slice(1) : hashText;
        }
        return qa;
    }

    function buildCoordinate(state) {
        var key = state.key || '';
        if (!state.group) {
            return '//';
        }
        if (!state.api) {
            return '//' + state.group + '/';
        }
        if (!key) {
            return '//' + state.group + '/' + state.api + (state.isListing ? '/' : '');
        }
        return '//' + state.group + '/' + state.api + '//' + key + (state.isListing ? '/' : '');
    }

    function buildCanonicalHpprHref(state) {
        return state.scheme + ':' + buildCoordinate(state) + serializeQa(state.qa);
    }

    function buildCompatPath(state) {
        var path = '/';
        if (state.api) {
            path += state.api;
        }
        if (state.key) {
            if (path.charAt(path.length - 1) !== '/') {
                path += '/';
            }
            path += state.key;
        }
        if (state.isListing || !state.key) {
            if (path.charAt(path.length - 1) !== '/') {
                path += '/';
            }
        }
        return path;
    }

    function buildCompatHpprHref(state) {
        return state.scheme + '://' + state.group + buildCompatPath(state) + projectedSearch(state.qa) + projectedHash(state.qa);
    }

    function applyCompatPathToState(state, pathname) {
        var trimmed = String(pathname || '/').replace(/^\/+/, '');
        var parts;
        state.api = '';
        state.key = '';
        state.isListing = true;
        if (trimmed === '') {
            return;
        }
        state.isListing = /\/$/.test(trimmed);
        if (state.isListing) {
            trimmed = trimmed.slice(0, -1);
        }
        if (trimmed === '') {
            return;
        }
        parts = trimmed.split('/');
        state.api = parts.shift() || '';
        state.key = parts.join('/');
    }

    function resolveCompatHpprTarget(input) {
        var current = currentHpprState();
        var target;
        var resolved;
        var base;
        var absoluteMatch;
        if (!current) {
            throw new TypeError('window.location compatibility mode could not read the current HAVI address');
        }
        rejectJsonqaInput(input);
        input = String(input);
        target = cloneHpprState(current);
        absoluteMatch = /^([A-Za-z][A-Za-z0-9+.-]*):/.exec(input);
        if (absoluteMatch && !isHpprSchemeName(absoluteMatch[1].toLowerCase())) {
            return {
                current: current,
                target: null,
                canonicalHref: input,
                compatHref: input,
                sameDocument: false,
                hashChanged: false,
                external: true
            };
        }
        if (absoluteMatch) {
            try {
                resolved = new URL(input);
            } catch (e) {
                throw new TypeError('Invalid HAVI compatibility URL: ' + input);
            }
            target.scheme = resolved.protocol.replace(/:$/, '').toLowerCase();
            target.group = resolved.host;
            applyCompatPathToState(target, resolved.pathname);
            target.qa = applyProjectedSearchAndHash(current.qa, resolved.search, resolved.hash);
        } else {
            base = 'https://' + (current.group || 'compat.invalid') + buildCompatPath(current) + projectedSearch(current.qa) + projectedHash(current.qa);
            try {
                resolved = new URL(input, base);
            } catch (e2) {
                throw new TypeError('Invalid HAVI compatibility URL: ' + input);
            }
            target.group = resolved.host;
            applyCompatPathToState(target, resolved.pathname);
            target.qa = applyProjectedSearchAndHash(current.qa, resolved.search, resolved.hash);
        }
        return {
            current: current,
            target: target,
            canonicalHref: buildCanonicalHpprHref(target),
            compatHref: buildCompatHpprHref(target),
            sameDocument: current.scheme === target.scheme &&
                current.group === target.group &&
                current.api === target.api &&
                current.key === target.key &&
                !!current.isListing === !!target.isListing,
            hashChanged: projectedHash(current.qa) !== projectedHash(target.qa),
            external: false
        };
    }

    function dispatchCompatHashChange(oldHref, newHref) {
        var event;
        if (oldHref === newHref) {
            return;
        }
        try {
            event = new HashChangeEvent('hashchange', {
                oldURL: oldHref,
                newURL: newHref
            });
        } catch (e) {
            event = document.createEvent('Event');
            event.initEvent('hashchange', false, false);
            try {
                event.oldURL = oldHref;
                event.newURL = newHref;
            } catch (e2) {}
        }
        window.dispatchEvent(event);
    }

    function currentCompatHistoryState() {
        if (isHpprPage()) {
            var hppr = requireHpprState();
            return {
                href: buildCompatHpprHref(hppr),
                hash: projectedHash(hppr.qa)
            };
        }
        if (isFilePage()) {
            var file = requireFileState();
            return {
                href: file.href,
                hash: file.hash || ''
            };
        }
        return null;
    }

    function syncCompatHistoryState() {
        var state = currentCompatHistoryState();
        compatHistoryHref = state ? state.href : null;
        compatHistoryHash = state ? state.hash : null;
    }

    function dispatchCompatTraversalHashChange() {
        var current = currentCompatHistoryState();
        var oldHref = compatHistoryHref;
        var oldHash = compatHistoryHash;
        compatHistoryHref = current ? current.href : null;
        compatHistoryHash = current ? current.hash : null;
        if (current && oldHref !== null && oldHash !== current.hash) {
            dispatchCompatHashChange(oldHref, current.href);
        }
    }

    function commitHpprSameDocument(result, replaceHistory, dispatchHash) {
        if (!result || result.external) {
            return;
        }
        if (!result.sameDocument) {
            return;
        }
        if (window.address && String(window.address.href || '') === result.canonicalHref) {
            return;
        }
        if (replaceHistory) {
            nativeHistoryReplaceState(null, '', result.canonicalHref);
        } else {
            nativeHistoryPushState(null, '', result.canonicalHref);
        }
        syncCompatHistoryState();
        if (dispatchHash && result.hashChanged) {
            dispatchCompatHashChange(buildCompatHpprHref(result.current), result.compatHref);
        }
    }

    function navigateCompatHppr(input, replaceHistory, dispatchHash) {
        var result = resolveCompatHpprTarget(input);
        if (result.external) {
            if (replaceHistory && nativeLocation && typeof nativeLocation.replace === 'function') {
                nativeLocation.replace(result.canonicalHref);
                return;
            }
            if (nativeLocation && typeof nativeLocation.assign === 'function') {
                nativeLocation.assign(result.canonicalHref);
                return;
            }
            if (window.address) {
                window.address.href = result.canonicalHref;
                return;
            }
            throw new TypeError('HAVI location compatibility cannot navigate without window.address');
        }
        if (result.sameDocument) {
            commitHpprSameDocument(result, replaceHistory, dispatchHash);
            return;
        }
        if (replaceHistory && nativeLocation && typeof nativeLocation.replace === 'function') {
            nativeLocation.replace(result.canonicalHref);
            return;
        }
        if (nativeLocation && typeof nativeLocation.assign === 'function') {
            nativeLocation.assign(result.canonicalHref);
            return;
        }
        if (window.address) {
            window.address.href = result.canonicalHref;
            return;
        }
        throw new TypeError('HAVI location compatibility cannot navigate without window.address');
    }

    function requireHpprState() {
        var state = currentHpprState();
        if (!state) {
            throw new TypeError('window.location compatibility mode could not read the current HAVI address');
        }
        return state;
    }

    function hpprProtocolSetter(value) {
        var state = requireHpprState();
        value = String(value || '').replace(/:$/, '').toLowerCase();
        if (!/^[a-z][a-z0-9+.-]*$/.test(value) || !isHpprSchemeName(value)) {
            throw new SyntaxError('Invalid HAVI scheme: ' + value);
        }
        state.scheme = value;
        navigateCompatHppr(buildCompatHpprHref(state), false, false);
    }

    function hpprHostSetter(value) {
        var state = requireHpprState();
        value = String(value || '');
        rejectJsonqaInput(value);
        if (value.indexOf(':') !== -1) {
            throw new TypeError('window.location.host cannot set direct routing in HAVI. Use window.address.href with {via:...}');
        }
        state.group = value;
        navigateCompatHppr(buildCompatHpprHref(state), false, false);
    }

    function hpprPathnameSetter(value) {
        var state = requireHpprState();
        rejectJsonqaInput(value);
        value = String(value || '');
        if (value === '') {
            value = '/';
        }
        if (value.charAt(0) !== '/') {
            throw new SyntaxError('window.location.pathname must start with / in HAVI compatibility mode');
        }
        applyCompatPathToState(state, value);
        navigateCompatHppr(buildCompatHpprHref(state), false, false);
    }

    function hpprSearchSetter(value) {
        var state = requireHpprState();
        var oldHref = buildCompatHpprHref(state);
        value = String(value || '');
        if (value !== '' && value.charAt(0) !== '?') {
            value = '?' + value;
        }
        state.qa = applyProjectedSearchAndHash(state.qa, value, projectedHash(state.qa));
        commitHpprSameDocument({
            current: requireHpprState(),
            target: state,
            canonicalHref: buildCanonicalHpprHref(state),
            compatHref: buildCompatHpprHref(state),
            sameDocument: true,
            hashChanged: false,
            external: false
        }, false, false);
        if (oldHref === buildCompatHpprHref(requireHpprState())) {
            return;
        }
    }

    function hpprHashSetter(value) {
        var state = requireHpprState();
        var current = cloneHpprState(state);
        value = String(value || '');
        if (value === '' || value === '#') {
            delete state.qa['#'];
        } else {
            state.qa['#'] = value.charAt(0) === '#' ? value.slice(1) : value;
        }
        commitHpprSameDocument({
            current: current,
            target: state,
            canonicalHref: buildCanonicalHpprHref(state),
            compatHref: buildCompatHpprHref(state),
            sameDocument: true,
            hashChanged: projectedHash(current.qa) !== projectedHash(state.qa),
            external: false
        }, false, true);
    }

    function makeHpprCompatLocation() {
        var obj = {};
        Object.defineProperties(obj, {
            href: {
                get: function() {
                    warnOnce();
                    return buildCompatHpprHref(requireHpprState());
                },
                set: function(value) {
                    warnOnce();
                    navigateCompatHppr(String(value), false, true);
                },
                configurable: true
            },
            protocol: {
                get: function() {
                    warnOnce();
                    return requireHpprState().scheme + ':';
                },
                set: function(value) {
                    warnOnce();
                    hpprProtocolSetter(value);
                },
                configurable: true
            },
            host: {
                get: function() {
                    warnOnce();
                    return requireHpprState().group;
                },
                set: function(value) {
                    warnOnce();
                    hpprHostSetter(value);
                },
                configurable: true
            },
            hostname: {
                get: function() {
                    warnOnce();
                    return requireHpprState().group;
                },
                set: function(value) {
                    warnOnce();
                    hpprHostSetter(value);
                },
                configurable: true
            },
            port: {
                get: function() {
                    warnOnce();
                    return '';
                },
                set: function() {
                    warnOnce();
                    throw new TypeError('window.location.port is not available in HAVI compatibility mode. Use window.address.href with {via:...}');
                },
                configurable: true
            },
            origin: {
                get: function() {
                    var state;
                    warnOnce();
                    state = requireHpprState();
                    return state.scheme + '://' + state.group;
                },
                configurable: true
            },
            pathname: {
                get: function() {
                    warnOnce();
                    return buildCompatPath(requireHpprState());
                },
                set: function(value) {
                    warnOnce();
                    hpprPathnameSetter(value);
                },
                configurable: true
            },
            search: {
                get: function() {
                    warnOnce();
                    return projectedSearch(requireHpprState().qa);
                },
                set: function(value) {
                    warnOnce();
                    hpprSearchSetter(value);
                },
                configurable: true
            },
            hash: {
                get: function() {
                    warnOnce();
                    return projectedHash(requireHpprState().qa);
                },
                set: function(value) {
                    warnOnce();
                    hpprHashSetter(value);
                },
                configurable: true
            }
        });
        obj.assign = function(value) {
            warnOnce();
            navigateCompatHppr(String(value), false, true);
        };
        obj.replace = function(value) {
            warnOnce();
            navigateCompatHppr(String(value), true, true);
        };
        obj.reload = function() {
            warnOnce();
            if (nativeLocation && typeof nativeLocation.reload === 'function') {
                nativeLocation.reload();
                return;
            }
            if (window.address) {
                window.address.href = String(window.address.href || '');
                return;
            }
            throw new TypeError('HAVI location compatibility cannot reload without window.address');
        };
        obj.toString = function() {
            warnOnce();
            return this.href;
        };
        if (typeof Symbol !== 'undefined' && Symbol.toStringTag) {
            try {
                Object.defineProperty(obj, Symbol.toStringTag, {
                    value: 'Location',
                    configurable: true
                });
            } catch (e) {}
        }
        return obj;
    }

    function currentFileState() {
        var addr;
        var resolved;
        var qa;
        try {
            addr = window.address;
            resolved = new URL(splitJsonqa(String(addr.href || ''))[0] || 'file:///');
        } catch (e) {
            return null;
        }
        qa = cloneQaValue(addr.qa) || {};
        var pathname = String(addr.pathname || resolved.pathname || '/');
        var search = projectedSearch(qa);
        var hash = projectedHash(qa);
        return {
            href: 'file://' + pathname + search + hash,
            protocol: 'file:',
            host: '',
            hostname: '',
            port: '',
            origin: 'file://',
            pathname: pathname,
            search: search,
            hash: hash,
            qa: qa,
            fragment: addr.fragment == null ? null : String(addr.fragment),
            isListing: !!addr.isListing
        };
    }

    function requireFileState() {
        var state = currentFileState();
        if (!state) {
            throw new TypeError('window.location compatibility mode could not read the current file URL');
        }
        return state;
    }

    function resolveCompatFileTarget(input) {
        var current = requireFileState();
        var resolved;
        var baseHref = splitJsonqa(current.href)[0] || current.href;
        rejectJsonqaInput(input);
        try {
            resolved = new URL(String(input), baseHref);
        } catch (e) {
            throw new TypeError('Invalid file compatibility URL: ' + input);
        }
        return {
            current: current,
            target: {
                href: resolved.href,
                protocol: resolved.protocol,
                host: resolved.host,
                hostname: resolved.hostname,
                port: resolved.port,
                origin: resolved.protocol === 'file:' ? 'file://' : resolved.origin,
                pathname: resolved.pathname || '/',
                search: resolved.search || '',
                hash: resolved.hash || ''
            },
            sameDocument: resolved.protocol === 'file:' &&
                resolved.pathname === current.pathname &&
                resolved.search === current.search,
            hashChanged: resolved.hash !== current.hash,
            external: resolved.protocol !== 'file:'
        };
    }

    function commitFileSameDocument(result, replaceHistory, dispatchHash) {
        var oldHref;
        var newHref;
        if (!result || !result.sameDocument) {
            return;
        }
        if (!nativeHistoryReplaceState || !nativeHistoryPushState) {
            throw new TypeError('HAVI location compatibility cannot update file history in this runtime');
        }
        oldHref = result.current.href;
        newHref = result.target.href;
        if (oldHref === newHref) {
            return;
        }
        if (replaceHistory) {
            nativeHistoryReplaceState(null, '', newHref);
        } else {
            nativeHistoryPushState(null, '', newHref);
        }
        syncCompatHistoryState();
        if (dispatchHash && result.hashChanged) {
            dispatchCompatHashChange(oldHref, newHref);
        }
    }

    function navigateCompatFile(input, replaceHistory, dispatchHash) {
        var result = resolveCompatFileTarget(input);
        if (result.sameDocument) {
            commitFileSameDocument(result, replaceHistory, dispatchHash);
            return;
        }
        if (nativeLocation) {
            if (replaceHistory && typeof nativeLocation.replace === 'function') {
                nativeLocation.replace(result.target.href);
                return;
            }
            if (typeof nativeLocation.assign === 'function') {
                nativeLocation.assign(result.target.href);
                return;
            }
            if ('href' in nativeLocation) {
                nativeLocation.href = result.target.href;
                return;
            }
        }
        throw new TypeError('HAVI location compatibility cannot navigate file URLs in this runtime');
    }

    function fileHashSetter(value) {
        var state = requireFileState();
        value = String(value || '');
        if (value !== '' && value.charAt(0) !== '#') {
            value = '#' + value;
        }
        navigateCompatFile('file://' + state.pathname + state.search + value, false, true);
    }

    function makeFileCompatLocation() {
        var obj = {};
        Object.defineProperties(obj, {
            href: {
                get: function() { warnOnce(); return requireFileState().href; },
                set: function(value) { warnOnce(); navigateCompatFile(String(value), false, true); },
                configurable: true
            },
            protocol: {
                get: function() { warnOnce(); return requireFileState().protocol; },
                set: function(value) { warnOnce(); navigateCompatFile(String(value) + requireFileState().pathname + requireFileState().search + requireFileState().hash, false, false); },
                configurable: true
            },
            host: {
                get: function() { warnOnce(); return requireFileState().host; },
                set: function() { warnOnce(); throw new TypeError('window.location.host is read-only for file URLs'); },
                configurable: true
            },
            hostname: {
                get: function() { warnOnce(); return requireFileState().hostname; },
                set: function() { warnOnce(); throw new TypeError('window.location.hostname is read-only for file URLs'); },
                configurable: true
            },
            port: {
                get: function() { warnOnce(); return requireFileState().port; },
                set: function() { warnOnce(); throw new TypeError('window.location.port is read-only for file URLs'); },
                configurable: true
            },
            origin: {
                get: function() { warnOnce(); return requireFileState().origin; },
                configurable: true
            },
            pathname: {
                get: function() { warnOnce(); return requireFileState().pathname; },
                set: function(value) {
                    var state;
                    warnOnce();
                    state = requireFileState();
                    value = String(value || '');
                    if (value === '') {
                        value = '/';
                    }
                    if (value.charAt(0) !== '/') {
                        throw new SyntaxError('window.location.pathname must start with / for file URLs');
                    }
                    navigateCompatFile('file://' + value + state.search + state.hash, false, false);
                },
                configurable: true
            },
            search: {
                get: function() { warnOnce(); return requireFileState().search; },
                set: function(value) {
                    var state;
                    warnOnce();
                    state = requireFileState();
                    value = String(value || '');
                    if (value !== '' && value.charAt(0) !== '?') {
                        value = '?' + value;
                    }
                    navigateCompatFile('file://' + state.pathname + value + state.hash, false, false);
                },
                configurable: true
            },
            hash: {
                get: function() { warnOnce(); return requireFileState().hash; },
                set: function(value) { warnOnce(); fileHashSetter(value); },
                configurable: true
            }
        });
        obj.assign = function(value) { warnOnce(); navigateCompatFile(String(value), false, true); };
        obj.replace = function(value) { warnOnce(); navigateCompatFile(String(value), true, true); };
        obj.reload = function() {
            warnOnce();
            if (nativeLocation && typeof nativeLocation.reload === 'function') {
                nativeLocation.reload();
                return;
            }
            throw new TypeError('HAVI location compatibility cannot reload file URLs in this runtime');
        };
        obj.toString = function() { warnOnce(); return this.href; };
        if (typeof Symbol !== 'undefined' && Symbol.toStringTag) {
            try {
                Object.defineProperty(obj, Symbol.toStringTag, {
                    value: 'Location',
                    configurable: true
                });
            } catch (e) {}
        }
        return obj;
    }

    function rewriteHpprHistoryUrl(url) {
        var result;
        if (url === undefined || url === null) {
            return url;
        }
        if (typeof url !== 'string') {
            return url;
        }
        warnOnce();
        result = resolveCompatHpprTarget(url);
        return result.canonicalHref;
    }

    function patchHistory() {
        var replaceImpl = nativeHistoryReplaceState;
        var pushImpl = nativeHistoryPushState;
        if (isHpprPage()) {
            if (nativeHistoryReplaceState) {
                replaceImpl = function(state, title, url) {
                    var result;
                    if (arguments.length < 3 || url === undefined || url === null || url === '') {
                        result = nativeHistoryReplaceState(state, title, url);
                    } else {
                        result = nativeHistoryReplaceState(state, title, rewriteHpprHistoryUrl(url));
                    }
                    syncCompatHistoryState();
                    return result;
                };
            }
            if (nativeHistoryPushState) {
                pushImpl = function(state, title, url) {
                    var result;
                    if (arguments.length < 3 || url === undefined || url === null || url === '') {
                        result = nativeHistoryPushState(state, title, url);
                    } else {
                        result = nativeHistoryPushState(state, title, rewriteHpprHistoryUrl(url));
                    }
                    syncCompatHistoryState();
                    return result;
                };
            }
        } else if (isFilePage()) {
            if (nativeHistoryReplaceState) {
                replaceImpl = function(state, title, url) {
                    var result = nativeHistoryReplaceState(state, title, url);
                    syncCompatHistoryState();
                    return result;
                };
            }
            if (nativeHistoryPushState) {
                pushImpl = function(state, title, url) {
                    var result = nativeHistoryPushState(state, title, url);
                    syncCompatHistoryState();
                    return result;
                };
            }
        }
        window.__haviCompatHistoryReplaceState = replaceImpl;
        window.__haviCompatHistoryPushState = pushImpl;
        if (replaceImpl) {
            try {
                Object.defineProperty(window.history, 'replaceState', {
                    value: replaceImpl,
                    configurable: true,
                    writable: true
                });
            } catch (e) {}
        }
        if (pushImpl) {
            try {
                Object.defineProperty(window.history, 'pushState', {
                    value: pushImpl,
                    configurable: true,
                    writable: true
                });
            } catch (e2) {}
        }
        if (!window.__haviCompatPopstateHooked) {
            window.__haviCompatPopstateHooked = true;
            try {
                window.addEventListener('popstate', function() {
                    dispatchCompatTraversalHashChange();
                });
            } catch (e3) {}
        }
        syncCompatHistoryState();
    }

    function ensureCompatLocation() {
        if (compatLocation) {
            return compatLocation;
        }
        if (isHpprPage()) {
            compatLocation = makeHpprCompatLocation();
        } else if (isFilePage() || nativeLocation) {
            compatLocation = makeFileCompatLocation();
        } else {
            throw new TypeError('window.location compatibility mode could not determine the current scheme');
        }
        window.__haviCompatLocation = compatLocation;
        patchHistory();
        return compatLocation;
    }
    window.__haviEnsureCompatLocation = ensureCompatLocation;

    try {
        Object.defineProperty(window, 'location', {
            get: function() {
                warnOnce();
                return ensureCompatLocation();
            },
            set: function(value) {
                warnOnce();
                ensureCompatLocation().href = String(value);
            },
            configurable: true
        });
    } catch (e) {}
    try {
        Object.defineProperty(document, 'location', {
            get: function() {
                warnOnce();
                return ensureCompatLocation();
            },
            set: function(value) {
                warnOnce();
                ensureCompatLocation().href = String(value);
            },
            configurable: true
        });
    } catch (e) {}

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

    window.__haviLocationCompatReady = true;
    window.__haviLocationCompatLoading = false;
})();
