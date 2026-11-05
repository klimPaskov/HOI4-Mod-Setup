import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import IntegrationsStep from "./IntegrationsStep";
import type { ImageChoice, ImageProvider } from "../protocol";
import { NO_LIVE_TESTING, NO_PORTRAITS } from "../integrations";

afterEach(cleanup);

const providers: ImageProvider[] = [
  { id: "hoi4_imagegen", display_name: "hoi4-imagegen", route_kind: "release_binary", backends: [
    { id: "chatgpt_login", display_name: "ChatGPT sign-in", default: true, credential_field: null, settings: [] },
    { id: "openai", display_name: "OpenAI", default: false, credential_field: "openai_api_key", settings: [] },
    { id: "stability", display_name: "Stability", default: false, credential_field: "stability_api_key", settings: [] },
  ] },
  { id: "none", display_name: "None", route_kind: "manual" },
  { id: "openai_api", display_name: "OpenAI Images", route_kind: "stdio_adapter", replaced_by: { backend: "openai" } },
  { id: "replicate", display_name: "Replicate", route_kind: "stdio_adapter" },
];

function renderStep(image: ImageChoice) {
  const onImage = vi.fn();
  render(<IntegrationsStep providers={providers} image={image} onImage={onImage} portraitsAvailable={false} portrait={NO_PORTRAITS} onPortrait={vi.fn()} liveProviders={[]} live={NO_LIVE_TESTING} onLive={vi.fn()} platform="windows" />);
  return onImage;
}

describe("web image step", () => {
  it("says so when the source version has no image generator", () => {
    render(<IntegrationsStep providers={[{ id: "none", display_name: "None", route_kind: "manual" }]} image={{ primary_provider: "none", additional_providers: [], route_settings: {} }} onImage={vi.fn()} portraitsAvailable={false} portrait={NO_PORTRAITS} onPortrait={vi.fn()} liveProviders={[]} live={NO_LIVE_TESTING} onLive={vi.fn()} platform="windows" />);
    expect(screen.getByText(/no image generator to install/)).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: /Install hoi4-imagegen/ })).toBeNull();
  });

  it("lists every backend the package publishes, with the ChatGPT sign-in first and selected", () => {
    renderStep({ primary_provider: "hoi4_imagegen", additional_providers: [], route_settings: {} });
    const backends = screen.getAllByRole("radio", { name: /ChatGPT sign-in|OpenAI|Stability/ });
    expect(backends.map((radio) => radio.getAttribute("value"))).toEqual(["chatgpt_login", "openai", "stability"]);
    expect(backends[0]).toBeChecked();
  });

  it("records the chosen backend and clears nothing else", () => {
    const onImage = renderStep({ primary_provider: "hoi4_imagegen", additional_providers: [], route_settings: {} });
    fireEvent.click(screen.getByRole("radio", { name: "Stability" }));
    expect(onImage).toHaveBeenCalledWith({ primary_provider: "hoi4_imagegen", additional_providers: [], route_settings: { hoi4_imagegen: { backend: "stability" } } });
  });

  it("keeps an older provider the project records and offers to move it only when a backend covers it", () => {
    const onImage = renderStep({ primary_provider: "openai_api", additional_providers: [], route_settings: { openai_api: { model: "gpt-image-2" } } });
    expect(screen.getByText(/It keeps working/)).toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: /Use hoi4-imagegen/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Move to hoi4-imagegen \(OpenAI\)/ }));
    expect(onImage).toHaveBeenCalledWith({ primary_provider: "hoi4_imagegen", additional_providers: [], route_settings: { hoi4_imagegen: { backend: "openai" } } });
  });

  it("leaves a provider that no backend covers yet exactly as it is", () => {
    renderStep({ primary_provider: "replicate", additional_providers: [], route_settings: {} });
    expect(screen.getByText(/does not cover it yet/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Move to hoi4-imagegen/ })).toBeNull();
  });
});
