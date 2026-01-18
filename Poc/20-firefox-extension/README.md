> ### Important
> To run this extension, I recommend not using the Tor Browser, as something can go wrong and mess the set configurations it already had. To avoid this, just use another browser, like Mullvad or Librewolf (their appimages will work too). Those are not configured automatically to use Tor, so an onion domain won't load by default (to evidence that the proxy is working). It is also mandatory to enable the "Allow extension in Private Tabs" permission request manually or else the extension won't be able to apply the settings.

## Testing
To temporarily run it, open the browser on `about:debugging#/runtime/this-firefox` (or clicking on the "Extensions" button on the toolbar, then on the gear and in "Debug Add-ons") and click on "Load temporary Add-on...". Select the `manifest.json` file. You should see a new extension added called "Datura extension", that will be removed when you close the browser.

## How it works
Click on the checkbox to toggle routing all traffic through 127.0.0.1:port. You can set the desired port on the numeric input just above it.

This extension makes a firefox-based regular browser able to connect to .onion domains (not tested with i2p yet), presenting no DNS/WebRTC leaks.