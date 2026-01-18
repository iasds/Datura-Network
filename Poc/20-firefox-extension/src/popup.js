const checkbox = document.getElementById('connect');
const p        = document.getElementById('p');
let settings   = browser.storage.local.get();

checkbox.checked = settings.connected;
p.value = settings.port===undefined ? 9050: settings.port;