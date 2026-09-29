import type { ProviderId, SecretName } from "@/lib/studio-types";

// Fixed product copy for the three ways to connect an AI that aren't a
// CLI tool. Only the words live here: whether a key is saved, which model
// is set and whether it works always come from real calls.

export interface ApiProviderDef {
  id: Extract<ProviderId, "anthropic-api" | "openai-api" | "local-model">;
  name: string;
  /** One plain sentence: what it is and what it costs. */
  blurb: string;
  /** The keychain entry for the key; null when there is no key. */
  secret: SecretName | null;
  keyUrl: string | null;
  keyLabel: string;
  modelPlaceholder: string;
  modelSuggestions: string[];
  modelHint: string;
}

export const API_PROVIDERS: ApiProviderDef[] = [
  {
    id: "anthropic-api",
    name: "Anthropic API",
    blurb:
      "Use your own Anthropic API key. You pay Anthropic for what you use; InfinaBox never charges you.",
    secret: "anthropic_api_key",
    keyUrl: "https://console.anthropic.com/settings/keys",
    keyLabel: "Get a key from Anthropic",
    modelPlaceholder: "For example: claude-sonnet-5",
    modelSuggestions: ["claude-sonnet-5", "claude-haiku-4-5-20251001"],
    modelHint: "Bigger models do better work; smaller ones are cheaper and quicker.",
  },
  {
    id: "openai-api",
    name: "OpenAI API",
    blurb:
      "Use your own OpenAI API key. You pay OpenAI for what you use; InfinaBox never charges you.",
    secret: "open_ai_api_key",
    keyUrl: "https://platform.openai.com/api-keys",
    keyLabel: "Get a key from OpenAI",
    modelPlaceholder: "For example: gpt-…",
    modelSuggestions: [],
    modelHint: "Check your provider's model list for the exact name.",
  },
  {
    id: "local-model",
    name: "A model on this computer",
    blurb:
      "Runs on your own computer, free and private. You need Ollama or LM Studio installed and a model downloaded.",
    secret: null,
    keyUrl: null,
    keyLabel: "",
    modelPlaceholder: "The name of a model you downloaded",
    modelSuggestions: [],
    modelHint: "Bigger models do much better; if results are poor, try a larger one.",
  },
];

export const LOCAL_DEFAULT_URL = "http://localhost:11434/v1";
export const LM_STUDIO_URL = "http://localhost:1234/v1";
