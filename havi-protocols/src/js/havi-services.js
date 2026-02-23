// havi:///services — Pylon service manager page
(function() {
    const statusEl = document.getElementById('pylonStatus');
    const servicesEl = document.getElementById('servicesList');
    const messageEl = document.getElementById('message');

    function showMessage(text, type) {
        messageEl.innerHTML = '<div class="message ' + type + '">' + text + '</div>';
        setTimeout(() => { messageEl.innerHTML = ''; }, 5000);
    }

    function stateColor(state) {
        switch (state) {
            case 'running': return '#27ae60';
            case 'starting': return '#f39c12';
            case 'stopping': return '#f39c12';
            case 'stopped': return '#c0392b';
            default: return '#888';
        }
    }

    async function pylonRequest(cmd, service) {
        // Use havi:///services/api endpoint via fetch
        const params = new URLSearchParams({ cmd: cmd });
        if (service) params.set('service', service);
        const resp = await fetch('havi:///services/api?' + params.toString());
        return await resp.json();
    }

    async function loadStatus() {
        try {
            const data = await pylonRequest('status');
            if (data.error) {
                statusEl.textContent = 'disconnected';
                statusEl.style.color = '#c0392b';
                servicesEl.innerHTML = '<p class="empty">' + data.error + '</p>';
                return;
            }

            statusEl.textContent = 'connected';
            statusEl.style.color = '#27ae60';

            const services = data.services || [];
            if (services.length === 0) {
                servicesEl.innerHTML = '<p class="empty">No services configured</p>';
                return;
            }

            let html = '';
            for (const svc of services) {
                const isRunning = svc.state === 'running';
                const portInfo = svc.port ? ' :' + svc.port : '';
                const pidInfo = svc.pid ? ' (pid ' + svc.pid + ')' : '';
                html += '<div class="list-item">';
                html += '<div>';
                html += '<span class="name">' + svc.name + '</span>';
                html += '<span style="color:' + stateColor(svc.state) + '; margin-left: 12px;">' + svc.state + '</span>';
                html += '<span style="color: #888; font-size: 0.85em;">' + portInfo + pidInfo + '</span>';
                html += '</div>';
                html += '<div>';
                if (isRunning) {
                    html += '<button class="danger btn-small" onclick="doStop(\'' + svc.name + '\')">Stop</button>';
                } else if (svc.state === 'stopped') {
                    html += '<button class="btn-small" onclick="doStart(\'' + svc.name + '\')">Start</button>';
                }
                html += '</div>';
                html += '</div>';
            }
            servicesEl.innerHTML = html;
        } catch (e) {
            statusEl.textContent = 'error';
            statusEl.style.color = '#c0392b';
            servicesEl.innerHTML = '<p class="error">' + e.message + '</p>';
        }
    }

    window.doStart = async function(name) {
        try {
            const data = await pylonRequest('start', name);
            if (data.error) {
                showMessage(data.error, 'error');
            } else {
                showMessage(name + ' starting...', 'success');
            }
            setTimeout(loadStatus, 1000);
        } catch (e) {
            showMessage(e.message, 'error');
        }
    };

    window.doStop = async function(name) {
        try {
            const data = await pylonRequest('stop', name);
            if (data.error) {
                showMessage(data.error, 'error');
            } else {
                showMessage(name + ' stopped', 'success');
            }
            setTimeout(loadStatus, 500);
        } catch (e) {
            showMessage(e.message, 'error');
        }
    };

    loadStatus();
    // Auto-refresh every 5 seconds
    setInterval(loadStatus, 5000);
})();
