// @ts-check

// havi:///services — Pylon service manager page
(function() {
    const statusEl = document.getElementById('pylonStatus');
    const servicesEl = document.getElementById('servicesList');
    const messageEl = document.getElementById('message');

    if (!statusEl || !servicesEl || !messageEl) return;

    const statusNode = /** @type {HTMLElement} */ (statusEl);
    const servicesNode = /** @type {HTMLElement} */ (servicesEl);
    const messageNode = /** @type {HTMLElement} */ (messageEl);

    /** @param {string} text @param {'error'|'success'} type */
    function showMessage(text, type) {
        messageNode.innerHTML = '<div class="message ' + type + '">' + text + '</div>';
        setTimeout(() => {
            messageNode.innerHTML = '';
        }, 5000);
    }

    /** @param {'status'|'start'|'stop'} cmd @param {string=} service */
    async function pylonRequest(cmd, service) {
        const params = new URLSearchParams({ cmd });
        if (service) params.set('service', service);
        const resp = await fetch('havi:///services/api?' + params.toString());
        return await resp.json();
    }

    async function loadStatus() {
        try {
            const data = await pylonRequest('status');
            if (data.error) {
                statusNode.textContent = 'disconnected';
                servicesNode.innerHTML = '<p class="empty">' + String(data.error) + '</p>';
                return;
            }

            statusNode.textContent = 'connected';

            const services = Array.isArray(data.services) ? data.services : [];
            if (services.length === 0) {
                servicesNode.innerHTML = '<p class="empty">No services configured</p>';
                return;
            }

            let html = '';
            for (const svc of services) {
                const state = String(svc.state || 'unknown');
                const name = String(svc.name || 'unknown');
                const isRunning = state === 'running';
                const portInfo = svc.port ? ' :' + String(svc.port) : '';
                const pidInfo = svc.pid ? ' (pid ' + String(svc.pid) + ')' : '';

                html += '<div class="list-item">';
                html += '<div>';
                html += '<span class="name">' + name + '</span>';
                html += '<span style="margin-left: 12px;">' + state + '</span>';
                html += '<span class="muted">' + portInfo + pidInfo + '</span>';
                html += '</div>';
                html += '<div>';
                if (isRunning) {
                    html += '<button class="danger btn-small" onclick="doStop(\'' + name + '\')">Stop</button>';
                } else if (state === 'stopped') {
                    html += '<button class="btn-small" onclick="doStart(\'' + name + '\')">Start</button>';
                }
                html += '</div>';
                html += '</div>';
            }

            servicesNode.innerHTML = html;
        } catch (e) {
            statusNode.textContent = 'error';
            const msg = e instanceof Error ? e.message : String(e);
            servicesNode.innerHTML = '<p class="error">' + msg + '</p>';
        }
    }

    /** @param {string} name */
    async function doStart(name) {
        try {
            const data = await pylonRequest('start', name);
            if (data.error) {
                showMessage(String(data.error), 'error');
            } else {
                showMessage(name + ' starting...', 'success');
            }
            setTimeout(loadStatus, 1000);
        } catch (e) {
            showMessage(e instanceof Error ? e.message : String(e), 'error');
        }
    }

    /** @param {string} name */
    async function doStop(name) {
        try {
            const data = await pylonRequest('stop', name);
            if (data.error) {
                showMessage(String(data.error), 'error');
            } else {
                showMessage(name + ' stopped', 'success');
            }
            setTimeout(loadStatus, 500);
        } catch (e) {
            showMessage(e instanceof Error ? e.message : String(e), 'error');
        }
    }

    /** @type {{ doStart?: (name: string) => Promise<void>, doStop?: (name: string) => Promise<void> }} */
    const g = /** @type {any} */ (window);
    g.doStart = doStart;
    g.doStop = doStop;

    loadStatus();
    setInterval(loadStatus, 5000);
})();
