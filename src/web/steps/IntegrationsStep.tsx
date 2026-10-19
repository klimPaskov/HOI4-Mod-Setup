// Image generation, portraits, and live testing: the desktop's Integrations
// choices, minus anything only the desktop can install or store.

import type { ImageChoice, ImageProvider, LiveTestingChoice, LiveTestingProvider, Platform, PortraitChoice } from "../protocol";
import { imageProviderNeeds, imageSettingKeys, imageSettingLabel, MAX_ADDITIONAL_IMAGE_PROVIDERS, NO_IMAGE_PROVIDER } from "../integrations";

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
  const primary = providers.find((provider) => provider.id === image.primary_provider);
  const selected = [image.primary_provider, ...image.additional_providers].filter((id) => id !== NO_IMAGE_PROVIDER);
  const setSetting = (id: string, key: string, value: string) => {
    const current = { ...(image.route_settings[id] ?? {}) };
    if (value) current[key] = value;
    else delete current[key];
    onImage({ ...image, route_settings: { ...image.route_settings, [id]: current } });
  };
  const clausewitz = liveProviders.find((provider) => provider.id === "clausewitz_mcp");
  return (
    <div className="stack">
      {providers.length > 0 && (
        <section className="panel form-panel" aria-labelledby="images-title">
          <h2 id="images-title" className="panel-title">Image generation</h2>
          <label className="field">
            <span className="field-label">Main image provider</span>
            <select className="text-input" value={image.primary_provider} onChange={(event) => onImage({ ...image, primary_provider: event.target.value, additional_providers: event.target.value === NO_IMAGE_PROVIDER ? [] : image.additional_providers.filter((id) => id !== event.target.value) })}>
              {providers.map((provider) => <option key={provider.id} value={provider.id}>{provider.display_name}</option>)}
            </select>
          </label>
          {primary && (
            <div className="web-provider-detail">
              {primary.description && <p className="muted">{primary.description}</p>}
              {(primary.requirements ?? []).length > 0 && <ul className="web-requirements">{primary.requirements!.map((item) => <li key={item}>{item}</li>)}</ul>}
              {imageProviderNeeds(primary) && <p className="status-review web-needs"><span className="status-symbol" aria-hidden="true">!</span> {imageProviderNeeds(primary)}</p>}
              {primary.documentation_url && <a className="external-link" href={primary.documentation_url} target="_blank" rel="noreferrer noopener">{primary.display_name} documentation</a>}
            </div>
          )}
          {image.primary_provider !== NO_IMAGE_PROVIDER && (
            <details className="web-additional">
              <summary className="details">More image providers{image.additional_providers.length > 0 ? ` (${image.additional_providers.length})` : ""}</summary>
              {providers.filter((provider) => provider.id !== image.primary_provider && provider.id !== NO_IMAGE_PROVIDER).map((provider) => {
                const checked = image.additional_providers.includes(provider.id);
                const full = !checked && image.additional_providers.length >= MAX_ADDITIONAL_IMAGE_PROVIDERS;
                return (
                  <label key={provider.id} className="check-row">
                    <input type="checkbox" checked={checked} disabled={full} onChange={(event) => onImage({ ...image, additional_providers: event.target.checked ? [...image.additional_providers, provider.id] : image.additional_providers.filter((id) => id !== provider.id) })} />
                    {provider.display_name}
                  </label>
                );
              })}
            </details>
          )}
          {selected.map((id) => {
            const provider = providers.find((item) => item.id === id);
            const keys = provider ? imageSettingKeys(provider) : [];
            if (!provider || keys.length === 0) return null;
            return (
              <fieldset key={id} className="web-provider-settings">
                <legend className="field-label">{provider.display_name} settings</legend>
                <div className="form-grid">
                  {keys.map((key) => key === "model" && (provider.supported_models ?? []).length > 0 ? (
                    <label key={key} className="field">
                      <span className="field-label">{imageSettingLabel(key)}</span>
                      <select className="text-input" value={image.route_settings[id]?.model ?? ""} onChange={(event) => setSetting(id, key, event.target.value)}>
                        <option value="">Provider default</option>
                        {provider.supported_models!.map((model) => <option key={model} value={model}>{model}</option>)}
                      </select>
                    </label>
                  ) : (
                    <label key={key} className="field">
                      <span className="field-label">{imageSettingLabel(key)}</span>
                      <input className="text-input mono" value={image.route_settings[id]?.[key] ?? ""} placeholder={key === "server_url" ? "http://127.0.0.1:8188" : undefined} onChange={(event) => setSetting(id, key, event.target.value.trim())} />
                    </label>
                  ))}
                </div>
              </fieldset>
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
