// Image generation, portraits, and live testing: the desktop's Integrations
// choices, minus anything only the desktop can install or store.

import type { ImageChoice, ImageProvider, LiveTestingChoice, LiveTestingProvider, Platform, PortraitChoice } from "../protocol";
import { imageSettingLabel, NO_IMAGE_PROVIDER } from "../integrations";

interface Props {
  providers: ImageProvider[];
  image: ImageChoice;
  onImage: (choice: ImageChoice) => void;
  portraitsAvailable: boolean;
  portrait: PortraitChoice;
  onPortrait: (choice: PortraitChoice) => void;
  liveProviders: LiveTestingProvider[];
  live: LiveTestingChoice;
  onLive: (choice: LiveTestingChoice) => void;
  platform: Platform;
}

export default function IntegrationsStep({ providers, image, onImage, portraitsAvailable, portrait, onPortrait, liveProviders, live, onLive, platform }: Props) {
  const imagegen = providers.find((provider) => provider.route_kind === "release_binary");
  const backends = imagegen?.backends ?? [];
  const recorded = imagegen ? image.route_settings[imagegen.id]?.backend : undefined;
  const backend = backends.find((option) => option.id === recorded) ?? backends.find((option) => option.default) ?? backends[0];
  const selected = [image.primary_provider, ...image.additional_providers].filter((id) => id !== NO_IMAGE_PROVIDER);
  const legacy = selected.map((id) => providers.find((provider) => provider.id === id)).filter((provider): provider is ImageProvider => Boolean(provider) && provider!.id !== imagegen?.id);
  const usingImagegen = Boolean(imagegen) && image.primary_provider === imagegen!.id;
  const setSetting = (id: string, key: string, value: string) => {
    const current = { ...(image.route_settings[id] ?? {}) };
    if (value) current[key] = value;
    else delete current[key];
    onImage({ ...image, route_settings: { ...image.route_settings, [id]: current } });
  };
  const chooseBackend = (id: string) => {
    if (!imagegen) return;
    const current: Record<string, string> = { ...(image.route_settings[imagegen.id] ?? {}), backend: id };
    // A setting belongs to the backend that declared it.
    const keep = new Set(backends.find((option) => option.id === id)?.settings.map((setting) => setting.key));
    for (const key of Object.keys(current)) if (key !== "backend" && !keep.has(key)) delete current[key];
    onImage({ ...image, route_settings: { ...image.route_settings, [imagegen.id]: current } });
  };
  const moveToImagegen = (legacyId: string, backendId: string) => {
    if (!imagegen) return;
    const settings = { ...image.route_settings };
    delete settings[legacyId];
    settings[imagegen.id] = { ...(settings[imagegen.id] ?? {}), backend: backendId };
    const wasPrimary = image.primary_provider === legacyId;
    onImage({
      primary_provider: wasPrimary ? imagegen.id : image.primary_provider,
      additional_providers: [...new Set([...image.additional_providers.filter((id) => id !== legacyId), ...(wasPrimary ? [] : [imagegen.id])])].filter((id) => id !== (wasPrimary ? imagegen.id : image.primary_provider)),
      route_settings: settings,
    });
  };
  const clausewitz = liveProviders.find((provider) => provider.id === "clausewitz_mcp");
  return (
    <div className="stack">
      {providers.length > 0 && (
        <section className="panel form-panel" aria-labelledby="images-title">
          <h2 id="images-title" className="panel-title">Image generation</h2>
          {legacy.length === 0 && (
            <div className="radio-row" role="radiogroup" aria-labelledby="images-title">
              {imagegen && (
                <label className="radio-option">
                  <input type="radio" name="web-image-generation" checked={usingImagegen} onChange={() => onImage({ ...image, primary_provider: imagegen.id, additional_providers: [] })} />
                  <span><strong>Use hoi4-imagegen</strong>{imagegen.description && <small>{imagegen.description}</small>}</span>
                </label>
              )}
              <label className="radio-option">
                <input type="radio" name="web-image-generation" checked={!usingImagegen} onChange={() => onImage({ ...image, primary_provider: NO_IMAGE_PROVIDER, additional_providers: [] })} />
                <span><strong>None, I make art myself</strong></span>
              </label>
            </div>
          )}
          {imagegen && usingImagegen && legacy.length === 0 && backends.length > 0 && (
            <div className="web-provider-detail">
              <span className="field-label" id="web-image-backend-label">Generate with</span>
              <div className="radio-row" role="radiogroup" aria-labelledby="web-image-backend-label">
                {backends.map((option) => (
                  <label className="radio-option" key={option.id}>
                    <input type="radio" name="web-image-backend" value={option.id} checked={backend?.id === option.id} onChange={() => chooseBackend(option.id)} />
                    <span><strong>{option.display_name}</strong>{option.default && <small>Default. Signs in with your ChatGPT account.</small>}{option.live_verified === false && <small>Untested with the real service.</small>}</span>
                  </label>
                ))}
              </div>
              {backend && backend.credential_field && <p className="status-review web-needs"><span className="status-symbol" aria-hidden="true">!</span> Its key is read from the operating-system credential store. A browser cannot save it there; add it later with the desktop app.</p>}
              {backend?.settings.map((setting) => (
                <label className="field" key={setting.key}>
                  <span className="field-label">{imageSettingLabel(setting.key)}{setting.required ? "" : " (optional)"}</span>
                  <input className="text-input mono" value={image.route_settings[imagegen.id]?.[setting.key] ?? ""} placeholder={setting.default ?? undefined} onChange={(event) => setSetting(imagegen.id, setting.key, event.target.value.trim())} />
                </label>
              ))}
            </div>
          )}
          {legacy.map((provider) => {
            const target = provider.replaced_by ? backends.find((option) => option.id === provider.replaced_by?.backend) : undefined;
            return (
              <div className="web-provider-detail" key={provider.id} role="group" aria-label={`${provider.display_name} image provider`}>
                <p><strong>{provider.display_name}</strong> is set up in this project. It keeps working.</p>
                {imagegen && target
                  ? <>
                    <p className="muted">hoi4-imagegen now covers it with the {target.display_name} backend. Its key stays where it is; add it for hoi4-imagegen with the desktop app.</p>
                    <button type="button" className="button secondary" onClick={() => moveToImagegen(provider.id, target.id)}>Move to hoi4-imagegen ({target.display_name})</button>
                  </>
                  : <p className="muted">hoi4-imagegen does not cover it yet. Nothing changes until it does.</p>}
              </div>
            );
          })}
        </section>
      )}

      {portraitsAvailable && (
        <section className="panel form-panel" aria-labelledby="portraits-title">
          <h2 id="portraits-title" className="panel-title">Portrait workflow</h2>
          <p className="muted">Leader portraits with ComfyUI. Credentials stay with the provider.</p>
          <div className="web-radio-group" role="radiogroup" aria-label="Portrait provider">
            {([["disabled", "Off"], ["cloud", "Comfy Cloud"], ["local", "ComfyUI on this computer"], ["runpod", "RunPod"]] as Array<[PortraitChoice["provider"], string]>).map(([value, label]) => (
              <label key={value} className="radio-row">
                <input type="radio" name="portrait-provider" checked={portrait.provider === value} onChange={() => onPortrait({ ...portrait, provider: value })} />
                {label}
              </label>
            ))}
          </div>
          {portrait.provider === "local" && (
            <div className="form-grid">
              <label className="field">
                <span className="field-label">ComfyUI folder (optional)</span>
                <input className="text-input mono" value={portrait.localComfyuiRoot ?? ""} onChange={(event) => onPortrait({ ...portrait, localComfyuiRoot: event.target.value })} />
              </label>
              <label className="field">
                <span className="field-label">Server address</span>
                <input className="text-input mono" value={portrait.localServerUrl ?? "http://127.0.0.1:8188"} onChange={(event) => onPortrait({ ...portrait, localServerUrl: event.target.value.trim() })} />
              </label>
            </div>
          )}
          {portrait.provider === "runpod" && (
            <div className="form-grid">
              <label className="field">
                <span className="field-label">RunPod address (optional)</span>
                <input className="text-input mono" value={portrait.runpodUrl ?? ""} placeholder="https://" onChange={(event) => onPortrait({ ...portrait, runpodUrl: event.target.value.trim() })} />
              </label>
              <label className="field">
                <span className="field-label">Workspace folder</span>
                <input className="text-input mono" value={portrait.runpodWorkspace ?? "/workspace/comfyui-hoi4-portraits"} onChange={(event) => onPortrait({ ...portrait, runpodWorkspace: event.target.value })} />
              </label>
            </div>
          )}
        </section>
      )}

      {clausewitz && (
        <section className="panel form-panel" aria-labelledby="live-title">
          <h2 id="live-title" className="panel-title">Live testing</h2>
          <p className="muted">{clausewitz.description ?? "Read the running game's state from your coding client."}</p>
          <div className="web-radio-group" role="radiogroup" aria-label="Live testing provider">
            <label className="radio-row">
              <input type="radio" name="live-testing-provider" checked={live.provider === "none"} onChange={() => onLive({ provider: "none", linux_ready: false, wsl_distribution: "", ssh_host: "" })} />
              Off
            </label>
            <label className="radio-row">
              <input type="radio" name="live-testing-provider" checked={live.provider === "clausewitz_mcp"} onChange={() => onLive({ ...live, provider: "clausewitz_mcp" })} />
              {clausewitz.display_name}
            </label>
            <label className="radio-row web-disabled-choice">
              <input type="radio" name="live-testing-provider" disabled checked={false} onChange={() => undefined} />
              HOI4 Live Tools <span className="muted">— desktop app only</span>
            </label>
          </div>
          {live.provider === "clausewitz_mcp" && (
            <>
              {(clausewitz.requirements ?? []).length > 0 && <ul className="web-requirements">{clausewitz.requirements!.map((item) => <li key={item}>{item}</li>)}</ul>}
              <div className="form-grid">
                {platform === "windows" && (
                  <label className="field">
                    <span className="field-label">WSL distribution (optional)</span>
                    <input className="text-input mono" value={live.wsl_distribution} onChange={(event) => onLive({ ...live, wsl_distribution: event.target.value.trim() })} />
                  </label>
                )}
                {platform === "macos" && (
                  <label className="field">
                    <span className="field-label">SSH host alias</span>
                    <input className="text-input mono" value={live.ssh_host} onChange={(event) => onLive({ ...live, ssh_host: event.target.value.trim() })} />
                  </label>
                )}
              </div>
              <label className="check-row">
                <input type="checkbox" checked={live.linux_ready} onChange={(event) => onLive({ ...live, linux_ready: event.target.checked })} />
                I confirm a compatible Linux game and Clausewitz MCP server are ready
              </label>
            </>
          )}
        </section>
      )}
    </div>
  );
}
