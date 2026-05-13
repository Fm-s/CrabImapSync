# Setting up OAuth2 for CrabImapSync

CrabImapSync uses the standard OAuth2 PKCE flow with a local browser
callback. You need to register an OAuth2 client at your provider's console
before first use.

## Gmail / Google Workspace

1. Go to <https://console.cloud.google.com/apis/credentials>.
2. Click **Create credentials** → **OAuth client ID**.
3. Application type: **Desktop app**.
4. Save the Client ID and Client Secret.
5. Enable the Gmail API: APIs & Services → Library → Gmail API → Enable.
6. Run:

   ```bash
   export GMAIL_OAUTH_SECRET='your-secret'
   crab-imap-sync ... \
     --dst-auth oauth2 --dst-oauth-provider gmail \
     --dst-oauth-client-id 'YOUR.apps.googleusercontent.com' \
     --dst-oauth-client-secret-env GMAIL_OAUTH_SECRET
   ```

A browser opens for consent. After approval the refresh token is stored in
your OS keyring (macOS Keychain / Linux Secret Service / Windows
Credential Manager) so subsequent runs skip the browser.

## Microsoft 365 / Outlook

1. Go to <https://entra.microsoft.com/> → App registrations → New registration.
2. Choose supported account types as appropriate for your tenant.
3. Redirect URI: **Public client (mobile & desktop)** → `http://localhost`.
4. Note the **Application (client) ID**.
5. API permissions → Add → Microsoft Graph (delegated):
   - `IMAP.AccessAsUser.All`
   - `offline_access`
6. Grant admin consent if your tenant requires it.
7. Run:

   ```bash
   crab-imap-sync ... \
     --dst-auth oauth2 --dst-oauth-provider microsoft \
     --dst-oauth-client-id 'YOUR-CLIENT-ID'
   ```

   Microsoft public clients don't have a client secret — omit
   `--dst-oauth-client-secret-env`.

## Custom (any OAuth2 provider)

```bash
crab-imap-sync ... \
  --dst-auth oauth2 --dst-oauth-provider custom \
  --dst-oauth-auth-url 'https://example.com/oauth/auth' \
  --dst-oauth-token-url 'https://example.com/oauth/token' \
  --dst-oauth-scope 'imap.read imap.write offline_access' \
  --dst-oauth-client-id 'cid' \
  --dst-oauth-client-secret-env CUSTOM_SECRET
```

## Disabling keyring

If you don't want refresh tokens persisted, add `--src-oauth-no-keyring` or
`--dst-oauth-no-keyring`. The browser flow will run every invocation.

## Source vs destination OAuth

Each side has its own `--src-oauth-*` and `--dst-oauth-*` flags. You can mix
LOGIN on one side and OAuth2 on the other — e.g., LOGIN on a cPanel source
and OAuth2 on a Gmail destination.
