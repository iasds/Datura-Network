const link = document.getElementById('link');
const chk  = document.getElementById('chk');
const p    = document.getElementById('p');

function route()
{
  const proxy = String(`127.0.0.1:${p.value}`);

  let proxySettings = {
    proxyType: "manual",
    socksVersion: 5,
    proxyDNS: true,
    socks: proxy
  };

  browser.proxy.settings
    .set({ value: proxySettings })
    .catch(error => {console.log(error);});

  browser.privacy.network.webRTCIPHandlingPolicy
    .set({value : "proxy_only"});

  msg("Connected to Datura");
}

function revert()
{
  browser.proxy.settings
    .set({ value: {proxyType: "none"} })
    .catch(error => {console.log(error);});

  browser.privacy.network.webRTCIPHandlingPolicy
    .set({value : "default_public_and_private_interfaces"});

  msg("Connect to Datura");
}

function msg(t)
{
  document.getElementById('str').textContent = t;
}

// Force user to allow extension in private tabs
if (!(await browser.extension.isAllowedIncognitoAccess()))
{
  msg("You must allow the extension to run in private tabs");
  link.textContent = "Go to about:addons";
  link.href        = "about:addons";
  chk.disabled     = true
  p.disabled       = true
}
else
{
  browser.storage.local.get().then( r => {
    p.value     = r.port===undefined ? 9050: r.port;
    chk.checked = r.connected;
    r.connected ? route(): revert();
  });
}

chk.addEventListener('change', function () {
  browser.storage.local.set({ connected: this.checked });
  this.checked ? route(): revert();
});

// Update saved port value in storage
p.addEventListener('change', function ()
{
  browser.storage.local.set({port: Number(p.value)});
  if (chk.checked)
  {
    browser.storage.local.set({ connected: false });
    chk.checked = false;
    msg("Reconnect to Datura");
  }
});