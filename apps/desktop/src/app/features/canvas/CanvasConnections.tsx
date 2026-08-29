import {
  getCanvasNodeSize,
  type CanvasConnection,
  type CanvasNode,
  type CanvasPoint,
} from "./canvas-state";

interface CanvasConnectionsProps {
  readonly connections: readonly CanvasConnection[];
  readonly nodes: readonly CanvasNode[];
  readonly selectedNodeId: string | null;
  readonly connectionSourceId: string | null;
}

/** Draws the persisted relationships underneath canvas cards. */
export function CanvasConnections({
  connections,
  nodes,
  selectedNodeId,
  connectionSourceId,
}: CanvasConnectionsProps) {
  const nodesById = new Map(nodes.map((node) => [node.id, node]));

  return (
    <svg
      className="canvas-connections"
      viewBox="0 0 2400 1600"
      preserveAspectRatio="none"
      aria-hidden="true"
      data-testid="canvas-connections"
    >
      {connections.map((connection) => {
        const source = nodesById.get(connection.sourceNodeId);
        const target = nodesById.get(connection.targetNodeId);
        if (!source || !target) {
          return null;
        }
        const geometry = createConnectionGeometry(source, target);
        const emphasized =
          selectedNodeId === source.id ||
          selectedNodeId === target.id ||
          connectionSourceId === source.id ||
          connectionSourceId === target.id;

        return (
          <g
            key={connection.id}
            className={
              emphasized
                ? "canvas-connection canvas-connection--emphasized"
                : "canvas-connection"
            }
            data-connection-id={connection.id}
          >
            <path d={geometry.path} vectorEffect="non-scaling-stroke" />
            <circle cx={geometry.source.x} cy={geometry.source.y} r="4" />
            <circle cx={geometry.target.x} cy={geometry.target.y} r="4" />
          </g>
        );
      })}
    </svg>
  );
}

interface ConnectionGeometry {
  readonly path: string;
  readonly source: CanvasPoint;
  readonly target: CanvasPoint;
}

function createConnectionGeometry(
  sourceNode: CanvasNode,
  targetNode: CanvasNode,
): ConnectionGeometry {
  const sourceCenter = nodeCenter(sourceNode);
  const targetCenter = nodeCenter(targetNode);
  const deltaX = targetCenter.x - sourceCenter.x;
  const deltaY = targetCenter.y - sourceCenter.y;
  const horizontal = Math.abs(deltaX) >= Math.abs(deltaY);
  const sourceSize = getCanvasNodeSize(sourceNode);
  const targetSize = getCanvasNodeSize(targetNode);

  if (horizontal) {
    const direction = deltaX >= 0 ? 1 : -1;
    const source = {
      x: sourceCenter.x + (sourceSize.width / 2) * direction,
      y: sourceCenter.y,
    };
    const target = {
      x: targetCenter.x - (targetSize.width / 2) * direction,
      y: targetCenter.y,
    };
    const controlOffset = Math.max(60, Math.abs(target.x - source.x) * 0.45);
    return {
      source,
      target,
      path: `M ${source.x} ${source.y} C ${source.x + controlOffset * direction} ${source.y}, ${target.x - controlOffset * direction} ${target.y}, ${target.x} ${target.y}`,
    };
  }

  const direction = deltaY >= 0 ? 1 : -1;
  const source = {
    x: sourceCenter.x,
    y: sourceCenter.y + (sourceSize.height / 2) * direction,
  };
  const target = {
    x: targetCenter.x,
    y: targetCenter.y - (targetSize.height / 2) * direction,
  };
  const controlOffset = Math.max(60, Math.abs(target.y - source.y) * 0.45);
  return {
    source,
    target,
    path: `M ${source.x} ${source.y} C ${source.x} ${source.y + controlOffset * direction}, ${target.x} ${target.y - controlOffset * direction}, ${target.x} ${target.y}`,
  };
}

function nodeCenter(node: CanvasNode): CanvasPoint {
  const size = getCanvasNodeSize(node);
  return {
    x: node.x + size.width / 2,
    y: node.y + size.height / 2,
  };
}
