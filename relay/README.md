# Free assistant relay

HOI4 Mod Setup's default setup assistant needs no account. The app sends one
structured chat request to this Cloudflare Worker, which forwards it to a
single free OpenRouter model with the maintainer's key. The key is stored as a
Worker secret and never reaches the app, the web page, or this repository.

The Worker accepts only what the app's core builds: the configured model, one
to four system or user messages, and an optional JSON Schema response format.
Requests are size-bounded, limited to six a minute per address, and browser
requests must come from `https://klimpaskov.github.io`. The desktop app sends
no `Origin` header and is subject to the same limits.

OpenRouter limits free models per account: 20 requests a minute, and 50 a
day until the account has bought at least 10 credits, then 1,000 a day. Every
user of the free route shares that allowance. A setup uses one to three
requests.

## Deploy

1. Create a Cloudflare API token with the **Edit Cloudflare Workers** template.
2. In this repository's settings, create an environment named
   `free-assistant` and add three secrets: `CLOUDFLARE_API_TOKEN`,
   `CLOUDFLARE_ACCOUNT_ID`, and `OPENROUTER_API_KEY`.
3. Run the **Deploy free assistant relay** workflow from the Actions tab.
4. Copy the `workers.dev` address from the deploy step and set `endpoint` in
   `src/free-assistant.json` to that address followed by
   `/v1/chat/completions`. The desktop app, the web edition, and the web
   page's Content Security Policy read it from there.

To rotate the key, update the environment secret and run the workflow again.
To change the model, update `MODEL` in `wrangler.toml` and `model` in
`src/free-assistant.json` together; a test keeps them in step.

## Test

```bash
pnpm test:relay
```
