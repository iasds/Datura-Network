const checkbox = document.getElementById('connect');
const link     = document.getElementById('link');
const str      = document.getElementById('str');
const p        = document.getElementById('p');

console.log("script.js loaded")

// Force user to allow extension in private tabs
if (!(await browser.extension.isAllowedIncognitoAccess()))
{
  checkbox.disabled = true
  p.disabled = true
  str.textContent = "You must allow the extension to run in private tabs";
  link.textContent = "Go to about:addons";
  link.href = "about:addons";
}

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

    str.textContent = "Connected to Datura";
}

function revert()
{
  browser.proxy.settings
    .set({ value: {proxyType: "none"} })
    .catch(error => {console.log(error);});

  browser.privacy.network.webRTCIPHandlingPolicy
    .set({value : "default_public_and_private_interfaces"});

  str.textContent = "Connect to Datura";
}

checkbox.addEventListener('change', function () {
  browser.storage.local.set({ connected: this.checked });
  this.checked ? route(): revert();
});

// Update saved port value in storage
p.addEventListener('change', function ()
{
  browser.storage.local.set({port: Number(p.value)});
  if (checkbox.checked)
  {
    browser.storage.local.set({ connected: false });
    checkbox.checked = false;
    str.textContent = "Reconnect to Datura";
  }
});