import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ExternalLink } from "@/components/studio/chat/ExternalLink";
import { credentialSet } from "@/lib/studio-api";
import type { GenProviderInfo, SecretName } from "@/lib/studio-types";
import { errorText } from "./assets-format";
import { Field, Note } from "./shared";

const SECRET_LABEL: Record<SecretName, string> = {
  cloudflare_account_id: "Cloudflare account ID",
  cloudflare_api_token: "Cloudflare API token",
  fish_audio_api_key: "Fish Audio API key",
  eleven_labs_api_key: "ElevenLabs API key",
  anthropic_api_key: "Anthropic API key",
  open_ai_api_key: "OpenAI API key",
};

/** An account ID isn't a secret the way a token is, but it's stored the same way. */
const isPlainId = (name: SecretName) => name === "cloudflare_account_id";

interface ConnectDialogProps {
  provider: GenProviderInfo | null;
  onClose: () => void;
  onConnected: () => void;
}

/** Asks for exactly the secrets the provider needs and saves them to the OS password storage. */
export function ConnectDialog({ provider, onClose, onConnected }: ConnectDialogProps) {
  const [values, setValues] = useState<Partial<Record<SecretName, string>>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setValues({});
    setError(null);
    setBusy(false);
  }, [provider?.id]);

  const complete = !!provider && provider.needs.every((n) => (values[n] ?? "").trim().length > 0);

  const save = async () => {
    if (!provider) return;
    setBusy(true);
    setError(null);
    try {
      for (const name of provider.needs) {
        await credentialSet(name, (values[name] ?? "").trim());
      }
      setValues({});
      onConnected();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={provider !== null} onOpenChange={(o) => !o && !busy && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Connect {provider?.name}</DialogTitle>
          <DialogDescription>
            Generating uses your own {provider?.name} account, so you pay them directly and InfinaBox never sees a bill.
          </DialogDescription>
        </DialogHeader>
        {provider && (
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault();
              if (complete && !busy) void save();
            }}
          >
            {provider.needs.map((name, i) => (
              <Field key={name} label={SECRET_LABEL[name]}>
                <Input
                  type={isPlainId(name) ? "text" : "password"}
                  autoComplete="off"
                  spellCheck={false}
                  autoFocus={i === 0}
                  value={values[name] ?? ""}
                  onChange={(e) => setValues((v) => ({ ...v, [name]: e.target.value }))}
                />
              </Field>
            ))}
            <p className="text-sm">
              Don't have one yet?{" "}
              <ExternalLink url={provider.signup_url}>Sign up at {provider.name}</ExternalLink>
            </p>
            {error && <Note tone="error">{error}</Note>}
            <DialogFooter>
              <Button type="button" variant="outline" onClick={onClose} disabled={busy}>
                Cancel
              </Button>
              <Button type="submit" disabled={!complete || busy}>
                {busy && <Loader2 className="animate-spin" />}
                Save
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}
