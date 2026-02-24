// @ts-check

// havi:///services — Pylon service manager page
(function() {
    const statusEl = document.getElementById('pylonStatus');
    const servicesEl = document.getElementById('servicesList');
    const listenersEl = document.getElementById('listenersList');
    const mountsEl = document.getElementById('mountsList');
    const natEl = document.getElementById('natInfo');
    const messageEl = document.getElementById('message');

    if (!statusEl || !servicesEl || !listenersEl || !mountsEl || !natEl || !messageEl) return;

    const statusNode = /** @type {HTMLElement} */ (statusEl);
    const servicesNode = /** @type {HTMLElement} */ (servicesEl);
    const listenersNode = /** @type {HTMLElement} */ (listenersEl);
    const mountsNode = /** @type {HTMLElement} */ (mountsEl);
    const natNode = /** @type {HTMLElement} */ (natEl);
    const messageNode = /** @type {HTMLElement} */ (messageEl);

    /** @type {any} */
    let lastStatus = null;

    /** @param {string} text @param {'error'|'success'} type */
    function showMessage(text, type) {
        messageNode.innerHTML = '<div class="message ' + type + '">' + text + '</div>';
        setTimeout(() => {
            messageNode.innerHTML = '';
        }, 5000);
    }

    /** @param {string} cmd @param {Record<string, string|number|boolean>=} params */
    async function pylonRequest(cmd, params) {
        const query = new URLSearchParams({ cmd });
        if (params) {
            for (const [k, v] of Object.entries(params)) {
                query.set(k, String(v));
            }
        }
        const resp = await fetch('havi:///services/api?' + query.toString());
        const data = await resp.json();
        if (!data.ok) {
            throw new Error(String(data.error || 'Unknown error'));
        }
        return data.data;
    }

    /** @param {string} raw */
    function parseArgs(raw) {
        /** @type {Record<string, string|number|boolean>} */
        const out = {};
        const text = raw.trim();
        if (!text) return out;
        const chunks = text.split('&');
        for (const chunk of chunks) {
            const [k, v] = chunk.split('=');
            if (!k || v === undefined) continue;
            const key = k.trim();
            const value = v.trim();
            if (!key || value === '') continue;
            if (value === 'true') out[key] = true;
            else if (value === 'false') out[key] = false;
            else if (/^-?\d+$/.test(value)) out[key] = Number(value);
            else out[key] = value;
        }
        return out;
    }

    /** @param {any} status */
    function renderSummary(status) {
        const mode = String(status.mode || 'unknown');
        const user = String(status.user || '-');
        const mountCount = Array.isArray(status.mounts) ? status.mounts.length : 0;
        statusNode.innerHTML = 'connected <span class="muted">(' + mode + ', ' + user + ', ' + mountCount + ' mounts)</span>';
    }

    /** @param {any} status */
    function renderServices(status) {
        const names = Object.keys(status)
            .filter(k => !['mode', 'user', 'mounts'].includes(k))
            .sort();

        if (names.length === 0) {
            servicesNode.innerHTML = '<p class="empty">No services configured</p>';
            return;
        }

        let html = '';
        for (const name of names) {
            const svc = status[name] || {};
            const state = String(svc.state || 'unknown');
            const isRunning = state === 'running' || state === 'external';
            const portInfo = svc.port ? ' :' + String(svc.port) : '';
            const pidInfo = svc.pid ? ' (pid ' + String(svc.pid) + ')' : '';
            const countInfo = svc.count ? ' (' + String(svc.count) + ')' : '';
            const listeners = Array.isArray(svc.listeners) ? svc.listeners.length : 0;
            const listenerInfo = listeners > 0 ? ' listeners=' + String(listeners) : '';

            html += '<div class="list-item">';
            html += '<div>';
            html += '<span class="name">' + name + '</span>';
            html += '<span style="margin-left: 12px;">' + state + '</span>';
            html += '<span class="muted">' + portInfo + pidInfo + countInfo + listenerInfo + '</span>';
            html += '</div>';
            html += '<div>';
            if (state !== 'external') {
                if (isRunning) {
                    html += '<button class="danger btn-small" onclick="doStop(\'' + name + '\')">Stop</button>';
                } else {
                    html += '<input type="text" id="args-' + name + '" placeholder="k=v&k2=v2" style="margin-right: 6px; width: 180px;">';
                    html += '<button class="btn-small" onclick="doStart(\'' + name + '\')">Start</button>';
                }
            }
            html += '</div>';
            html += '</div>';
        }

        servicesNode.innerHTML = html;
    }

    /** @param {any} status */
    function renderListeners(status) {
        const hpprd = status.hpprd || {};
        const listeners = Array.isArray(hpprd.listeners) ? hpprd.listeners : [];
        const state = String(hpprd.state || 'unknown');

        if (state === 'external') {
            listenersNode.innerHTML = '<p class="empty">hpprd is external in remote mode</p>';
            return;
        }

        if (listeners.length === 0) {
            listenersNode.innerHTML = '<p class="empty">No listeners</p>';
            return;
        }

        let html = '';
        for (const listener of listeners) {
            const id = String(listener);
            html += '<div class="list-item">';
            html += '<div><span class="name">' + id + '</span></div>';
            html += '<div><button class="danger btn-small" onclick="removeListener(\'' + id + '\')">Remove</button></div>';
            html += '</div>';
        }
        listenersNode.innerHTML = html;
    }

    /** @param {any} status */
    function renderMounts(status) {
        const mounts = Array.isArray(status.mounts) ? status.mounts : [];
        if (mounts.length === 0) {
            mountsNode.innerHTML = '<p class="empty">No active mounts</p>';
            return;
        }

        let html = '';
        for (const mount of mounts) {
            const mountpoint = String(mount.mountpoint || '');
            const device = String(mount.device || '-');
            const fstype = String(mount.fstype || '-');
            html += '<div class="list-item">';
            html += '<div><span class="name">' + mountpoint + '</span> <span class="muted">' + fstype + ' ' + device + '</span></div>';
            html += '<div><button class="danger btn-small" onclick="unmountPath(\'' + mountpoint + '\')">Unmount</button></div>';
            html += '</div>';
        }
        mountsNode.innerHTML = html;
    }

    /** @param {any} status */
    function renderNat(status) {
        const nat = status['hppr-nat'] || {};
        const state = String(nat.state || 'unknown');
        const gateway = nat.gateway ? String(nat.gateway) : '-';
        const extIp = nat.external_ip ? String(nat.external_ip) : '-';
        const proto = nat.protocol ? String(nat.protocol) : '-';
        const mappings = Array.isArray(nat.mappings) ? nat.mappings : [];

        let html = '';
        html += '<div class="muted">state=' + state + ' gateway=' + gateway + ' external_ip=' + extIp + ' protocol=' + proto + '</div>';
        if (mappings.length === 0) {
            html += '<p class="empty">No mappings</p>';
        } else {
            for (const mapping of mappings) {
                const port = String(mapping.port || '-');
                const mapProto = String(mapping.proto || '-');
                const extPort = mapping.external_port ? String(mapping.external_port) : '-';
                html += '<div class="list-item"><div><span class="name">' + port + '/' + mapProto + '</span> <span class="muted">→ ' + extPort + '</span></div></div>';
            }
        }
        natNode.innerHTML = html;
    }

    async function loadStatus() {
        try {
            const status = await pylonRequest('status');
            lastStatus = status;
            const g = /** @type {any} */ (window);
            g._lastStatus = status;
            renderSummary(status);
            renderServices(status);
            renderListeners(status);
            renderMounts(status);
            renderNat(status);
        } catch (e) {
            statusNode.textContent = 'disconnected';
            const msg = e instanceof Error ? e.message : String(e);
            servicesNode.innerHTML = '<p class="empty">' + msg + '</p>';
            listenersNode.innerHTML = '<p class="empty">' + msg + '</p>';
            mountsNode.innerHTML = '<p class="empty">' + msg + '</p>';
            natNode.innerHTML = '<p class="empty">' + msg + '</p>';
        }
    }

    /** @param {string} name */
    async function doStart(name) {
        try {
            const argsEl = document.getElementById('args-' + name);
            const raw = argsEl instanceof HTMLInputElement ? argsEl.value : '';
            const params = parseArgs(raw);
            params.service = name;
            await pylonRequest('start', params);
            showMessage(name + ' started', 'success');
            setTimeout(loadStatus, 600);
        } catch (e) {
            showMessage(e instanceof Error ? e.message : String(e), 'error');
        }
    }

    /** @param {string} name */
    async function doStop(name) {
        try {
            await pylonRequest('stop', { service: name });
            showMessage(name + ' stopped', 'success');
            setTimeout(loadStatus, 400);
        } catch (e) {
            showMessage(e instanceof Error ? e.message : String(e), 'error');
        }
    }

    async function addListener() {
        const bindInput = document.getElementById('listenerBind');
        const bind = bindInput instanceof HTMLInputElement ? bindInput.value.trim() : '';
        if (!bind) {
            showMessage('listener bind is required', 'error');
            return;
        }

        try {
            await pylonRequest('listen', { bind });
            showMessage('listener added: ' + bind, 'success');
            if (bindInput instanceof HTMLInputElement) bindInput.value = '';
            setTimeout(loadStatus, 300);
        } catch (e) {
            showMessage(e instanceof Error ? e.message : String(e), 'error');
        }
    }

    /** @param {string} bind */
    async function removeListener(bind) {
        try {
            await pylonRequest('unlisten', { bind });
            showMessage('listener removed: ' + bind, 'success');
            setTimeout(loadStatus, 300);
        } catch (e) {
            showMessage(e instanceof Error ? e.message : String(e), 'error');
        }
    }

    async function createMount() {
        const mountpointEl = document.getElementById('mountpoint');
        const rootEl = document.getElementById('mountRoot');
        const signerEl = document.getElementById('mountSigner');
        const rwEl = document.getElementById('mountRw');

        const mountpoint = mountpointEl instanceof HTMLInputElement ? mountpointEl.value.trim() : '';
        const root = rootEl instanceof HTMLInputElement ? rootEl.value.trim() : '';
        const signer = signerEl instanceof HTMLInputElement ? signerEl.value.trim() : '';
        const rw = rwEl instanceof HTMLInputElement ? rwEl.checked : false;

        /** @type {Record<string, string|number|boolean>} */
        const params = {};
        if (mountpoint) params.mountpoint = mountpoint;
        if (root) params.root = root;
        if (signer) params.signer = signer;
        if (rw) params.rw = true;

        try {
            const data = await pylonRequest('mount', params);
            const mountedPath = data && data.mountpoint ? String(data.mountpoint) : (mountpoint || '(default)');
            showMessage('mounted: ' + mountedPath, 'success');
            setTimeout(loadStatus, 500);
        } catch (e) {
            showMessage(e instanceof Error ? e.message : String(e), 'error');
        }
    }

    /** @param {string} mountpoint */
    async function unmountPath(mountpoint) {
        try {
            await pylonRequest('unmount', { mountpoint });
            showMessage('unmounted: ' + mountpoint, 'success');
            setTimeout(loadStatus, 500);
        } catch (e) {
            showMessage(e instanceof Error ? e.message : String(e), 'error');
        }
    }

    /** @type {{ doStart?: (name: string) => Promise<void>, doStop?: (name: string) => Promise<void>, addListener?: () => Promise<void>, removeListener?: (bind: string) => Promise<void>, createMount?: () => Promise<void>, unmountPath?: (mountpoint: string) => Promise<void>, loadStatus?: () => Promise<void>, _lastStatus?: any }} */
    const g = /** @type {any} */ (window);
    g.doStart = doStart;
    g.doStop = doStop;
    g.addListener = addListener;
    g.removeListener = removeListener;
    g.createMount = createMount;
    g.unmountPath = unmountPath;
    g.loadStatus = loadStatus;
    g._lastStatus = lastStatus;

    loadStatus();
    setInterval(loadStatus, 5000);
})();
