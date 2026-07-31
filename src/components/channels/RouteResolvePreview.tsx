import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Loader2, Route } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { extractErrorMessage } from "@/utils/errorUtils";
import { useResolveRoute } from "@/lib/query/channels";
import type { InterfaceKind, RouteResolveResponse } from "@/types/channel";

const APP_TYPES = ["claude", "codex", "gemini"] as const;
const INTERFACE_KINDS: InterfaceKind[] = [
  "anthropic_messages",
  "openai_chat_completions",
  "openai_responses",
  "gemini_native",
];

export function RouteResolvePreview() {
  const { t } = useTranslation();
  const resolveRoute = useResolveRoute();

  const [appType, setAppType] = useState<string>("claude");
  const [requestedModel, setRequestedModel] = useState("");
  const [interfaceKind, setInterfaceKind] =
    useState<string>("anthropic_messages");
  const [routeGroup, setRouteGroup] = useState("");
  const [result, setResult] = useState<RouteResolveResponse | null>(null);

  const handleResolve = async () => {
    try {
      const res = await resolveRoute.mutateAsync({
        appType,
        requestedModel: requestedModel.trim() || null,
        interfaceKind: interfaceKind || null,
        routeGroup: routeGroup.trim() || null,
      });
      setResult(res);
    } catch (err) {
      toast.error(extractErrorMessage(err) || t("channel.toast.error"));
    }
  };

  return (
    <div className="border-t border-border/50 pt-5 space-y-3">
      <div className="flex items-center gap-2">
        <Route className="h-4 w-4 text-indigo-500" />
        <h4 className="text-sm font-semibold">{t("channel.route.title")}</h4>
      </div>
      <p className="text-xs text-muted-foreground">
        {t("channel.route.description")}
      </p>

      <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
        <div className="space-y-1">
          <Label className="text-xs">{t("channel.filter.app")}</Label>
          <Select value={appType} onValueChange={setAppType}>
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {APP_TYPES.map((app) => (
                <SelectItem key={app} value={app}>
                  {app}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1">
          <Label className="text-xs">{t("channel.route.requestedModel")}</Label>
          <Input
            value={requestedModel}
            onChange={(e) => setRequestedModel(e.target.value)}
            placeholder="claude-opus-4-8"
          />
        </div>
        <div className="space-y-1">
          <Label className="text-xs">{t("channel.route.interfaceKind")}</Label>
          <Select value={interfaceKind} onValueChange={setInterfaceKind}>
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {INTERFACE_KINDS.map((kind) => (
                <SelectItem key={kind} value={kind}>
                  {kind}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1">
          <Label className="text-xs">{t("channel.route.routeGroup")}</Label>
          <Input
            value={routeGroup}
            onChange={(e) => setRouteGroup(e.target.value)}
            placeholder="default"
          />
        </div>
      </div>

      <Button
        size="sm"
        onClick={() => void handleResolve()}
        disabled={resolveRoute.isPending}
      >
        {resolveRoute.isPending && <Loader2 className="h-4 w-4 animate-spin" />}
        {t("channel.route.resolve")}
      </Button>

      {result && (
        <div className="space-y-3">
          <div className="flex items-center gap-2 text-xs">
            <Badge variant="secondary">
              {t("channel.route.source")}: {result.source}
            </Badge>
          </div>

          <div>
            <h5 className="text-xs font-semibold mb-1">
              {t("channel.route.candidates", {
                count: result.candidates.length,
              })}
            </h5>
            {result.candidates.length === 0 ? (
              <p className="text-xs text-muted-foreground">
                {t("channel.route.noCandidates")}
              </p>
            ) : (
              <ul className="text-xs space-y-1">
                {result.candidates.map((c, i) => (
                  <li
                    key={`${c.channelId}-${i}`}
                    className="flex items-center gap-2"
                  >
                    <Badge variant="outline">{i + 1}</Badge>
                    <span className="font-medium">{c.channelName}</span>
                    <span className="text-muted-foreground truncate">
                      {c.baseUrl}
                    </span>
                    <span className="text-muted-foreground">
                      p{c.priority}/w{c.weight}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>

          {result.rejected.length > 0 && (
            <div>
              <h5 className="text-xs font-semibold mb-1">
                {t("channel.route.rejected", {
                  count: result.rejected.length,
                })}
              </h5>
              <ul className="text-xs space-y-1 text-muted-foreground max-h-40 overflow-y-auto">
                {result.rejected.map((r, i) => (
                  <li key={`${r.channelId}-${i}`}>
                    <span className="font-medium text-foreground">
                      {r.channelName}
                    </span>
                    {": "}
                    {r.reasons.join(", ")}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
