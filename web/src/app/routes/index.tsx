import { createFileRoute } from "@tanstack/react-router";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/ui/card";

export const Route = createFileRoute("/")({
  component: HomeRoute,
});

function HomeRoute() {
  const relayUrl = import.meta.env.VITE_RELAY_URL || "ws://localhost:3000";

  return (
    <div className="flex flex-1 items-center justify-center p-4">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle>Relay</CardTitle>
          <CardDescription>Connected relay endpoint</CardDescription>
        </CardHeader>
        <CardContent>
          <code
            className="text-sm font-mono text-muted-foreground"
            data-testid="relay-url"
          >
            {relayUrl}
          </code>
        </CardContent>
      </Card>
    </div>
  );
}
