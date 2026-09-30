import { useCallback, useEffect, useId, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { CheckCircle2, KeyRound, Loader2, MessageCircle } from "lucide-react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ExternalLink } from "@/components/studio/chat/ExternalLink";
import { appSettingsGet, appSettingsSet, credentialClear, credentialSet, credentialStatus } from "@/lib/studio-api";
import { fadeRise, fadeTransition } from "@/lib/motion";
import type { AppSettings } from "@/lib/studio-types";
import { LM_STUDIO_URL, LOCAL_DEFAULT_URL, type ApiProviderDef } from "./api-providers";
import { TestResultView, type TestState } from "./ProviderCard";

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

interface ApiProviderCardProps {
  def: ApiProviderDef;
  recommended: boolean;
  inUse: boolean;
  /** The last settings the panel read; the card re-reads before writing. */
  settings: AppSettings | null;
  /** Bumps when the panel re-checks, so the key status is asked again. */
  checkToken: number;
  test: TestState | null;
  choosing: boolean;
  onSettingsSaved: (settings: AppSettings) => void;
  onTest: () => void;
  onUse: () => void;
}

/** A way to connect an AI that isn't a CLI tool: an API key, or a model
 * running on this computer. What's shown as status comes from real calls
 * (`credential_status`, `app_settings_get`, the Test button); a call that
 * fails is shown as the plain text it failed with. A saved key is never
 * read back or shown. */
export function ApiProviderCard({
  def,
  recommended,
  inUse,
  settings,
  checkToken,
  test,
  choosing,
  onSettingsSaved,
  onTest,
  onUse,
}: ApiProviderCardProps) {
  const isLocal = def.secret === null;
  const uid = useId();
  const testing = test?.status === "running";

  // --- The key (Anthropic / OpenAI only) ---
  // `undefined` = not asked yet, `null` = the ask failed.
  const [keySaved, setKeySaved] = useState<boolean | null | undefined>(undefined);
  const [keyStatusError, setKeyStatusError] = useState<string | null>(null);
  const [keyDraft, setKeyDraft] = useState("");
  const [keyBusy, setKeyBusy] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);
  const [confirmRemove, setConfirmRemove] = useState(false);

  const secret = def.secret;
  useEffect(() => {
    if (!secret) return;
    let cancelled = false;
    credentialStatus([secret]).then(
      (rows) => {
        if (cancelled) return;
        setKeySaved(rows.find((r) => r.name === secret)?.connected ?? false);
        setKeyStatusError(null);
      },
      (err) => {
        if (cancelled) return;
        setKeySaved(null);
        setKeyStatusError(errorText(err));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [secret, checkToken]);

  async function saveKey() {
    if (!secret || keyDraft.trim() === "") return;
    setKeyBusy(true);
    setKeyError(null);
    try {
      await credentialSet(secret, keyDraft.trim());
      // Gone from this screen the moment it's stored.
      setKeyDraft("");
      const rows = await credentialStatus([secret]).catch(() => null);
      setKeySaved(rows ? (rows.find((r) => r.name === secret)?.connected ?? true) : true);
      setKeyStatusError(null);
    } catch (err) {
      setKeyError(`Couldn't save the key: ${errorText(err)}`);
    } finally {
      setKeyBusy(false);
    }
  }

  async function removeKey() {
    if (!secret) return;
    setConfirmRemove(false);
    setKeyBusy(true);
    setKeyError(null);
    try {
      await credentialClear(secret);
      setKeySaved(false);
      setKeyStatusError(null);
    } catch (err) {
      setKeyError(`Couldn't remove the key: ${errorText(err)}`);
    } finally {
      setKeyBusy(false);
    }
  }

  // --- The model (and, for a local model, the server address) ---
  const savedModel = settings?.models[def.id]?.model ?? "";
  const savedUrl = settings?.models[def.id]?.base_url ?? "";
  const [modelDraft, setModelDraft] = useState(savedModel);
  const [urlDraft, setUrlDraft] = useState(savedUrl || (isLocal ? LOCAL_DEFAULT_URL : ""));
  const [modelBusy, setModelBusy] = useState(false);
  const [modelError, setModelError] = useState<string | null>(null);
  const [modelSaved, setModelSaved] = useState(false);

  // Follow what's saved (first read, or another save) without clobbering
  // something the person is typing: drafts only reset when the saved value moves.
  useEffect(() => {
    setModelDraft(savedModel);
  }, [savedModel]);
  useEffect(() => {
    setUrlDraft(savedUrl || (isLocal ? LOCAL_DEFAULT_URL : ""));
  }, [savedUrl, isLocal]);

  const modelDirty =
    modelDraft.trim() !== savedModel || (isLocal && urlDraft.trim() !== (savedUrl || ""));

  const saveModel = useCallback(async () => {
    setModelBusy(true);
    setModelError(null);
    setModelSaved(false);
    try {
      // Fresh read so every other setting is kept as it is now.
      const current = await appSettingsGet();
      const existing = current.models[def.id];
      const saved = await appSettingsSet({
        ...current,
        models: {
          ...current.models,
          [def.id]: {
            base_url: isLocal ? urlDraft.trim() || null : (existing?.base_url ?? null),
            model: modelDraft.trim() || null,
          },
        },
      });
      onSettingsSaved(saved);
      setModelSaved(true);
    } catch (err) {
      setModelError(`Couldn't save: ${errorText(err)}`);
    } finally {
      setModelBusy(false);
    }
  }, [def.id, isLocal, modelDraft, urlDraft, onSettingsSaved]);

  const hasKey = isLocal || keySaved === true;
  const hasModel = savedModel !== "";
  const canUse = hasKey && hasModel && !modelDirty;
  const listId = `${uid}-models`;

  return (
    <div
      data-testid={`provider-${def.id}`}
      className={`flex flex-col gap-3 rounded-lg border bg-background p-3 ${
        inUse ? "border-emerald-500/50" : "border-border"
      }`}
    >
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm font-medium text-foreground/90">{def.name}</span>
        {recommended && <Badge variant="secondary">Recommended</Badge>}
        {inUse && (
          <Badge variant="outline" className="border-emerald-500/40 text-emerald-400">
            <CheckCircle2 />
            In use
          </Badge>
        )}
      </div>
      {isLocal ? (
        <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs">
          <ExternalLink url="https://ollama.com">Get Ollama</ExternalLink>
          <ExternalLink url="https://lmstudio.ai">Get LM Studio</ExternalLink>
        </div>
      ) : (
        <div className="flex flex-col gap-2">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
            <span className="text-muted-foreground">API key</span>
            <span
              data-testid={`key-status-${def.id}`}
              className={keySaved === true ? "text-emerald-400" : "text-foreground/90"}
            >
              {keySaved === undefined
                ? "Checking…"
                : keySaved === null
                  ? "Couldn't check"
                  : keySaved
                    ? "Key saved"
                    : "No key yet"}
            </span>
            {keySaved === true && (
              <Button
                type="button"
                size="xs"
                variant="ghost"
                data-testid={`key-remove-${def.id}`}
                disabled={keyBusy}
                onClick={() => setConfirmRemove(true)}
              >
                Remove key
              </Button>
            )}
            {def.keyUrl && (
              <span className="ml-auto">
                <ExternalLink url={def.keyUrl}>{def.keyLabel}</ExternalLink>
              </span>
            )}
          </div>
          {keyStatusError && (
            <p className="text-xs break-words text-destructive" data-testid={`key-status-error-${def.id}`}>
              Couldn't check for a saved key: {keyStatusError}
            </p>
          )}
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void saveKey();
            }}
          >
            <Input
              type="password"
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="off"
              spellCheck={false}
              name={`${def.id}-key`}
              aria-label={`Paste your ${def.name} key`}
              data-testid={`key-input-${def.id}`}
              placeholder={keySaved === true ? "Paste a new key to replace it" : "Paste your API key"}
              value={keyDraft}
              disabled={keyBusy}
              onChange={(e) => setKeyDraft(e.target.value)}
            />
            <Button
              type="submit"
              size="sm"
              variant="secondary"
              data-testid={`key-save-${def.id}`}
              disabled={keyBusy || keyDraft.trim() === ""}
            >
              {keyBusy ? <Loader2 data-icon="inline-start" className="animate-spin" /> : <KeyRound data-icon="inline-start" />}
              Save
            </Button>
          </form>
          {keyError && (
            <p className="text-xs break-words text-destructive" data-testid={`key-error-${def.id}`}>
              {keyError}
            </p>
          )}
          </div>
      )}

      <form
        className="flex flex-col gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (modelDirty && !modelBusy) void saveModel();
        }}
      >
        {isLocal && (
          <label className="flex flex-col gap-1 text-xs text-muted-foreground">
            Server address
            <Input
              type="text"
              inputMode="url"
              autoComplete="off"
              spellCheck={false}
              data-testid="local-url-input"
              placeholder={LOCAL_DEFAULT_URL}
              value={urlDraft}
              onChange={(e) => setUrlDraft(e.target.value)}
            />
            <span className="flex flex-wrap gap-x-3">
              <button
                type="button"
                className="underline underline-offset-3 hover:text-foreground"
                onClick={() => setUrlDraft(LOCAL_DEFAULT_URL)}
              >
                Use Ollama's address
              </button>
              <button
                type="button"
                data-testid="local-url-lmstudio"
                className="underline underline-offset-3 hover:text-foreground"
                onClick={() => setUrlDraft(LM_STUDIO_URL)}
              >
                Use LM Studio's address
              </button>
            </span>
          </label>
        )}
        <label className="flex flex-col gap-1 text-xs text-muted-foreground">
          {isLocal ? "Model name" : "Model"}
          <div className="flex gap-2">
            <Input
              type="text"
              autoComplete="off"
              spellCheck={false}
              list={def.modelSuggestions.length > 0 ? listId : undefined}
              data-testid={`model-input-${def.id}`}
              placeholder={def.modelPlaceholder}
              value={modelDraft}
              onChange={(e) => {
                setModelDraft(e.target.value);
                setModelSaved(false);
              }}
            />
            <Button
              type="submit"
              size="sm"
              variant="secondary"
              data-testid={`model-save-${def.id}`}
              disabled={!modelDirty || modelBusy}
            >
              {modelBusy ? "Saving…" : "Save"}
            </Button>
          </div>
          {def.modelSuggestions.length > 0 && (
            <datalist id={listId}>
              {def.modelSuggestions.map((m) => (
                <option key={m} value={m} />
              ))}
            </datalist>
          )}
        </label>
        {modelSaved && !modelDirty && (
          <p className="text-xs text-emerald-400" data-testid={`model-saved-${def.id}`}>
            Saved.
          </p>
        )}
        {modelError && (
          <p className="text-xs break-words text-destructive" data-testid={`model-error-${def.id}`}>
            {modelError}
          </p>
        )}
      </form>

      <div className="flex flex-wrap items-center gap-2">
        <Button
          type="button"
          size="sm"
          variant="secondary"
          data-testid={`provider-test-${def.id}`}
          disabled={testing}
          onClick={onTest}
        >
          {testing ? (
            <Loader2 data-icon="inline-start" className="animate-spin" />
          ) : (
            <MessageCircle data-icon="inline-start" />
          )}
          {testing ? "Testing…" : "Test"}
        </Button>
        {!inUse && (
          <Button
            type="button"
            size="sm"
            variant="outline"
            data-testid={`provider-use-${def.id}`}
            disabled={!canUse || choosing}
            onClick={onUse}
          >
            {choosing ? "Saving…" : "Use this one"}
          </Button>
        )}
        {!inUse && !canUse && (
          <span className="text-xs text-muted-foreground">
            {!hasKey
              ? "Save a key and pick a model to use it."
              : !hasModel
                ? "Pick a model to use it."
                : "Save your changes to use it."}
          </span>
        )}
      </div>

      <AnimatePresence initial={false}>
        {test && test.status !== "running" && (
          <motion.div key="test" {...fadeRise} transition={fadeTransition}>
            <TestResultView test={test} provider={def.id} />
          </motion.div>
        )}
      </AnimatePresence>

      <AlertDialog open={confirmRemove} onOpenChange={setConfirmRemove}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove your {def.name} key?</AlertDialogTitle>
            <AlertDialogDescription>
              InfinaBox will forget the key on this computer. The key itself still works at{" "}
              {def.name.replace(" API", "")}; you can paste it again any time.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Keep it</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={() => void removeKey()}>
              Remove key
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
