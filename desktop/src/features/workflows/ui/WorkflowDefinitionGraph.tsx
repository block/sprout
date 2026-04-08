import {
  Background,
  BackgroundVariant,
  Controls,
  MarkerType,
  Position,
  ReactFlow,
  useReactFlow,
  type Edge,
  type Node,
} from "@xyflow/react";
import * as React from "react";

import "@xyflow/react/dist/style.css";

import { cn } from "@/shared/lib/cn";
import {
  asRecord,
  asString,
  getStepDetails,
  getStepTitle,
  getTriggerDetails,
  getTriggerTitle,
} from "./workflowDefinitionNodeInfo";

type WorkflowDefinitionGraphProps = {
  definition: Record<string, unknown>;
  selectedNodeId: string | null;
  onSelectedNodeIdChange: (id: string | null) => void;
  /** Merged onto the outer wrapper (e.g. full-bleed: no radius, no frame border). */
  className?: string;
  /** Use bottom-left when a right overlay covers the default controls corner. */
  controlsPosition?: "bottom-left" | "bottom-right";
};

/**
 * Positions are proportional to this reference width so spacing stays tight when
 * `nodeWidth` changes (including canvas-percent sizing).
 */
const LAYOUT_REF_NODE_W = 200;

function GraphNodeLabel({
  eyebrow,
  title,
  subtitle,
  details,
  nodeWidth,
}: {
  eyebrow: string;
  title: string;
  subtitle?: string | null;
  details: string[];
  nodeWidth: number;
}) {
  /** Scales with canvas-derived node width (cap keeps type readable). */
  const rootPx = Math.max(6, Math.min(8, 5.1 + nodeWidth * 0.018));

  return (
    <div
      className="min-w-0 w-full max-w-full px-[0.55em] py-[0.58em] text-left"
      style={{ fontSize: `${rootPx}px`, lineHeight: 1.32 }}
    >
      <p className="text-[0.86em] font-semibold uppercase tracking-[0.13em] text-muted-foreground">
        {eyebrow}
      </p>
      <p className="mt-[0.32em] text-[1.05em] font-semibold leading-tight text-foreground">
        {title}
      </p>
      {subtitle ? (
        <p className="mt-[0.32em] font-mono text-[0.9em] leading-tight text-muted-foreground">
          {subtitle}
        </p>
      ) : null}
      {details.length > 0 ? (
        <div className="mt-[0.5em] space-y-[0.18em]">
          {details.map((detail) => (
            <p
              className="text-[0.94em] leading-snug text-muted-foreground"
              key={detail}
            >
              {detail}
            </p>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function computeNodeWidth(canvasW: number, canvasH: number): number {
  const w = Math.max(canvasW, 1);
  const h = Math.max(canvasH, 1);
  const basis = Math.min(w, h);
  /** ~20% of the smaller canvas dimension, bounded so nodes stay compact but legible */
  return Math.round(Math.min(142, Math.max(76, basis * 0.2)));
}

function buildGraph(
  definition: Record<string, unknown>,
  selectedNodeId: string | null,
  nodeWidth: number,
): { nodes: Node[]; edges: Edge[] } {
  const trigger = asRecord(definition.trigger);
  const rawSteps = Array.isArray(definition.steps) ? definition.steps : [];
  const steps = rawSteps.map((step) => asRecord(step)).filter(Boolean) as Array<
    Record<string, unknown>
  >;

  const r = nodeWidth / LAYOUT_REF_NODE_W;
  const nodeX = 26 * r;
  const triggerY = 14 * r;
  const stepY0 = 118 * r;
  const stepDeltaY = 108 * r;

  const selectedBorder = "2px solid hsl(var(--primary) / 0.85)";
  const defaultBorderTrigger = "1px solid hsl(var(--primary) / 0.35)";
  const defaultBorderStep = "1px solid hsl(var(--border))";

  const borderRadius = Math.max(6, Math.round(11 * r));
  const boxShadow = `0 ${Math.max(2, Math.round(3 * r))}px ${Math.max(8, Math.round(12 * r))}px hsl(var(--foreground) / 0.05)`;
  const markerSize = Math.max(7, Math.round(10 * r));

  const nodes: Node[] = [
    {
      id: "trigger",
      position: { x: nodeX, y: triggerY },
      sourcePosition: Position.Bottom,
      data: {
        label: (
          <GraphNodeLabel
            details={getTriggerDetails(trigger)}
            eyebrow="Trigger"
            nodeWidth={nodeWidth}
            title={getTriggerTitle(trigger)}
          />
        ),
      },
      style: {
        width: nodeWidth,
        borderRadius,
        border:
          selectedNodeId === "trigger" ? selectedBorder : defaultBorderTrigger,
        background: "hsl(var(--card))",
        color: "hsl(var(--card-foreground))",
        boxShadow,
        padding: 0,
        cursor: "pointer",
      },
    },
  ];

  const edges: Edge[] = [];
  let previousNodeId = "trigger";

  steps.forEach((step, index) => {
    const stepId = asString(step.id) ?? `step_${index + 1}`;
    const nodeId = `step-${stepId}`;
    const isSelected = selectedNodeId === nodeId;

    nodes.push({
      id: nodeId,
      position: { x: nodeX, y: stepY0 + index * stepDeltaY },
      sourcePosition: Position.Bottom,
      targetPosition: Position.Top,
      data: {
        label: (
          <GraphNodeLabel
            details={getStepDetails(step)}
            eyebrow={`Step ${index + 1}`}
            nodeWidth={nodeWidth}
            subtitle={stepId}
            title={getStepTitle(step, index)}
          />
        ),
      },
      style: {
        width: nodeWidth,
        borderRadius,
        border: isSelected ? selectedBorder : defaultBorderStep,
        background: "hsl(var(--card))",
        color: "hsl(var(--card-foreground))",
        boxShadow,
        padding: 0,
        cursor: "pointer",
      },
    });

    edges.push({
      id: `edge-${previousNodeId}-${nodeId}`,
      source: previousNodeId,
      target: nodeId,
      type: "smoothstep",
      markerEnd: {
        type: MarkerType.ArrowClosed,
        width: markerSize,
        height: markerSize,
        color: "hsl(var(--muted-foreground))",
      },
      style: {
        stroke: "hsl(var(--muted-foreground))",
        strokeWidth: Math.max(1, Math.min(1.35, 0.75 + nodeWidth / 220)),
      },
    });

    previousNodeId = nodeId;
  });

  return { nodes, edges };
}

function RefitWhenLayoutChanges({ layoutKey }: { layoutKey: string }) {
  const { fitView } = useReactFlow();
  React.useEffect(() => {
    void layoutKey;
    const id = requestAnimationFrame(() => {
      fitView({ duration: 0, padding: 0.18 });
    });
    return () => cancelAnimationFrame(id);
  }, [fitView, layoutKey]);
  return null;
}

export function WorkflowDefinitionGraph({
  definition,
  selectedNodeId,
  onSelectedNodeIdChange,
  className,
  controlsPosition = "bottom-right",
}: WorkflowDefinitionGraphProps) {
  const containerRef = React.useRef<HTMLDivElement>(null);
  const [canvasSize, setCanvasSize] = React.useState({ w: 400, h: 280 });

  React.useLayoutEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      setCanvasSize({ w: width, h: height });
    });
    ro.observe(el);
    setCanvasSize({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  const nodeWidth = React.useMemo(
    () => computeNodeWidth(canvasSize.w, canvasSize.h),
    [canvasSize.h, canvasSize.w],
  );

  const stepCount = React.useMemo(() => {
    const raw = definition.steps;
    return Array.isArray(raw) ? raw.length : 0;
  }, [definition.steps]);

  const definitionDigest = React.useMemo(
    () => JSON.stringify(definition),
    [definition],
  );

  const refitLayoutKey = `${nodeWidth}-${stepCount}-${definitionDigest}`;

  const { nodes, edges } = React.useMemo(
    () => buildGraph(definition, selectedNodeId, nodeWidth),
    [definition, selectedNodeId, nodeWidth],
  );

  const onNodeClick = React.useCallback(
    (_event: React.MouseEvent, node: Node) => {
      onSelectedNodeIdChange(node.id);
    },
    [onSelectedNodeIdChange],
  );

  const onPaneClick = React.useCallback(() => {
    onSelectedNodeIdChange(null);
  }, [onSelectedNodeIdChange]);

  return (
    <div
      className={cn(
        "h-full min-h-[12rem] w-full overflow-hidden rounded-xl border border-border/70 bg-muted/15",
        className,
      )}
      ref={containerRef}
    >
      <ReactFlow
        edges={edges}
        elementsSelectable={false}
        fitView
        fitViewOptions={{ padding: 0.18 }}
        nodes={nodes}
        nodesConnectable={false}
        nodesDraggable={false}
        onNodeClick={onNodeClick}
        onPaneClick={onPaneClick}
        panOnDrag
        proOptions={{ hideAttribution: true }}
        selectNodesOnDrag={false}
      >
        <RefitWhenLayoutChanges layoutKey={refitLayoutKey} />
        <Background gap={20} size={1} variant={BackgroundVariant.Dots} />
        <Controls position={controlsPosition} showInteractive={false} />
      </ReactFlow>
    </div>
  );
}
