// HAVI: Define stubs for disabled web APIs on hppr* schemes.
// Evaluated before parser starts; invisible to page scripts.
(function() {
    function disabled(msg) { return { get: function() { throw new TypeError(msg); }, set: function() { throw new TypeError(msg); }, configurable: true }; }
    try { Object.defineProperty(window, 'location', disabled('window.location is disabled in HAVI. Use window.address')); } catch(e) {}
    try { Object.defineProperty(document, 'location', disabled('document.location is disabled in HAVI. Use window.address')); } catch(e) {}
    var net = ['WebSocket', 'XMLHttpRequest', 'EventSource'];
    for (var i = 0; i < net.length; i++) {
        try { Object.defineProperty(window, net[i], disabled(net[i] + ' is disabled in HAVI. Use window.home or window.route')); } catch(e) {}
    }
    var doc = ['write', 'writeln', 'open', 'close'];
    for (var i = 0; i < doc.length; i++) {
        (function(n) {
            try { Object.defineProperty(document, n, { value: function() { throw new TypeError('document.' + n + '() is disabled in HAVI'); }, writable: false, configurable: true }); } catch(e) {}
        })(doc[i]);
    }
})();
