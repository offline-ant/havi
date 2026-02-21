// @ts-check
/// <reference path="havi.d.ts" />

/** @param {string} text @param {boolean} isError */
function showMessage(text, isError) {
    const el = document.getElementById('message');
    if (!el) return;
    el.className = 'message ' + (isError ? 'error' : 'success');
    el.textContent = text;
    el.style.display = 'block';
    setTimeout(() => el.style.display = 'none', 5000);
}

async function scanRequests() {
    const list = document.getElementById('requestsList');
    if (!list) return;
    /** @type {{ ring1Name: string, cmd: string, payload: string, hash: string }[]} */
    const requests = [];

    try {
        if (!window.ring0) throw new Error('ring0 unavailable');
        const ring1Names = await window.ring0.list('//repo/admin/ring1/');

        for (const nameEntry of ring1Names) {
            const name = nameEntry.replace(/\/$/, '');
            if (['ring0', 'anyone', 'guest'].includes(name)) continue;

            const cmds = ['LIST', 'HEADERS', 'ADD'];
            for (const cmd of cmds) {
                try {
                    const reqUrc = '//repo/admin/ring1/' + name + '/' + cmd + '/|';
                    const packet = await window.ring0.get(reqUrc);
                    let hasReply = false;
                    try {
                        await window.ring0.get('//repo/admin/ring1/' + name + '/' + cmd + '/reply/|');
                        hasReply = true;
                    } catch (_e) {
                        // No reply yet
                    }

                    if (!hasReply) {
                        const payload = packet.text();
                        requests.push({
                            ring1Name: name,
                            cmd: cmd,
                            payload: payload.trim(),
                            hash: packet.hash,
                        });
                    }
                } catch (_e) {
                    // No request for this cmd
                }
            }
        }
    } catch (e) {
        list.innerHTML = '<p class="empty">Failed to scan: ' + (e instanceof Error ? e.message : String(e)) + '</p>';
        return;
    }

    if (requests.length === 0) {
        list.innerHTML = '<p class="empty">No pending proxy requests</p>';
        return;
    }

    let html = '';
    for (const req of requests) {
        const escapedPayload = req.payload.replace(/</g, '&lt;').replace(/>/g, '&gt;');
        html += `
            <div class="request-card" id="req-${req.ring1Name}-${req.cmd}">
                <div class="request-header">
                    <div>
                        <span class="request-ring1">${req.ring1Name}</span>
                        <span class="request-cmd">${req.cmd}</span>
                    </div>
                    <div class="btn-group">
                        <button class="btn-small" onclick="approveRequest('${req.ring1Name}', '${req.cmd}', '${req.hash}')">Approve</button>
                        <button class="btn-small danger" onclick="denyRequest('${req.ring1Name}', '${req.cmd}', '${req.hash}')">Deny</button>
                    </div>
                </div>
                <div class="request-detail">Target: ${escapedPayload}</div>
            </div>
        `;
    }

    list.innerHTML = html;
}

/**
 * @param {string} ring1Name
 * @param {string} cmd
 * @param {string} reqHash
 * @param {string} data
 */
async function writeProxyReply(ring1Name, cmd, reqHash, data) {
    if (!window.ring0) throw new Error('ring0 unavailable');
    const now = Math.floor(Date.now() / 1000) + ':00000000';
    await window.ring0.add({
        headers: [
            'Group: repo',
            'App: admin',
            'Location: ring1/' + ring1Name + '/' + cmd + '/reply',
            'TAI: ' + now,
            '+Link: request ' + reqHash
        ],
        data: data
    });
}

/**
 * @param {string} ring1Name
 * @param {string} cmd
 * @param {string} reqHash
 */
async function approveRequest(ring1Name, cmd, reqHash) {
    try {
        if (!window.ring0) throw new Error('ring0 unavailable');
        const reqPacket = await window.ring0.get('//repo/admin/ring1/' + ring1Name + '/' + cmd + '/|');
        const payload = reqPacket.text();
        /** @type {string} */
        let resultPayload;
        switch (cmd) {
            case 'LIST': {
                const targetCoord = payload.trim();
                if (!targetCoord) {
                    await writeProxyReply(ring1Name, cmd, reqHash, 'ERROR INVALID missing target coordinate');
                    showMessage('Request has no target coordinate', true);
                    return;
                }
                const entries = await window.ring0.list(targetCoord);
                resultPayload = entries.join('\n');
                break;
            }
            case 'HEADERS': {
                const targetCoord = payload.trim();
                if (!targetCoord) {
                    await writeProxyReply(ring1Name, cmd, reqHash, 'ERROR INVALID missing target coordinate');
                    showMessage('Request has no target coordinate', true);
                    return;
                }
                const hdrs = await window.ring0.headers(targetCoord);
                resultPayload = hdrs.join('\n');
                break;
            }
            case 'ADD': {
                const splitIndex = payload.indexOf('\n\n');
                const headersText = splitIndex === -1 ? payload : payload.slice(0, splitIndex);
                const dataText = splitIndex === -1 ? '' : payload.slice(splitIndex + 2);
                const headers = headersText.split('\n').filter(/** @param {string} line */ line => line.length > 0);

                const hasGroup = headers.some(/** @param {string} line */ line => line.startsWith('Group:'));
                const hasApp = headers.some(/** @param {string} line */ line => line.startsWith('App:'));
                const hasLocation = headers.some(/** @param {string} line */ line => line.startsWith('Location:'));
                const startsWithMarkline = headersText.startsWith('\u{1f5a7}:');

                if (startsWithMarkline || !hasGroup || !hasApp || !hasLocation) {
                    await writeProxyReply(ring1Name, cmd, reqHash, 'ERROR INVALID unsupported ADD payload');
                    showMessage('ADD payload must be headers-only with Group/App/Location (complete packets are not supported by proxy)', true);
                    return;
                }

                const hashes = await window.ring0.add({
                    headers: headers,
                    data: dataText
                });
                resultPayload = hashes.join('\n');
                break;
            }
            default:
                showMessage('Unknown command: ' + cmd, true);
                return;
        }

        await writeProxyReply(ring1Name, cmd, reqHash, resultPayload);

        showMessage('Approved: ' + ring1Name + ' ' + cmd, false);
        scanRequests();
    } catch (e) {
        try {
            await writeProxyReply(ring1Name, cmd, reqHash, 'ERROR FAILED ' + (e instanceof Error ? e.message : String(e)));
        } catch (_err) {
            // Fall through to UI error
        }
        showMessage('Failed to approve: ' + (e instanceof Error ? e.message : String(e)), true);
    }
}

/**
 * @param {string} ring1Name
 * @param {string} cmd
 * @param {string} reqHash
 */
async function denyRequest(ring1Name, cmd, reqHash) {
    try {
        await writeProxyReply(ring1Name, cmd, reqHash, 'ERROR FORBIDDEN denied by admin');
        showMessage('Denied: ' + ring1Name + ' ' + cmd, false);
        scanRequests();
    } catch (e) {
        showMessage('Failed to deny: ' + (e instanceof Error ? e.message : String(e)), true);
    }
}

/** @type {WatchSocket|null} */
let watchSocket = null;
/** @type {ReturnType<typeof setTimeout>|undefined} */
let _scanTimer;

function startWatch() {
    try {
        if (!window.ring0) return;
        watchSocket = window.ring0.watch('//repo/admin/ring1/');
        watchSocket.onmessage = () => {
            clearTimeout(_scanTimer);
            _scanTimer = setTimeout(scanRequests, 500);
        };
        watchSocket.onerror = () => {
            const indicator = document.getElementById('watchIndicator');
            if (indicator) indicator.className = 'status-indicator status-error';
        };
        watchSocket.onclose = () => {
            const indicator = document.getElementById('watchIndicator');
            if (indicator) indicator.className = 'status-indicator status-error';
            setTimeout(startWatch, 3000);
        };
    } catch (e) {
        console.error('Watch failed:', e);
    }
}

scanRequests();
startWatch();
