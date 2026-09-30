import { useEffect, useRef, useState } from "react";
import {
  Box3,
  DirectionalLight,
  HemisphereLight,
  Mesh,
  PerspectiveCamera,
  Scene,
  Vector3,
  WebGLRenderer,
  type Material,
  type Object3D,
  type Texture,
} from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { base64ToBytes } from "./assets-format";
import { Note } from "./shared";

// The 3D viewer. This file is the only place `three` is imported, and
// AssetPreview loads it with React.lazy, so `three` ships as its own chunk
// and never weighs down the main bundle.

interface ModelPreviewProps {
  base64: string;
  /** "glb" or "gltf". */
  extension: string;
}

const SEPARATE_FILES_NOTE = "This model uses separate files; open it in Godot to see it.";

/** True when a .gltf refers to buffers/images that aren't embedded in the file. */
function usesSeparateFiles(json: string): boolean {
  try {
    const doc = JSON.parse(json) as { buffers?: { uri?: string }[]; images?: { uri?: string }[] };
    const external = (list?: { uri?: string }[]) => (list ?? []).some((e) => e.uri && !e.uri.startsWith("data:"));
    return external(doc.buffers) || external(doc.images);
  } catch {
    return false;
  }
}

function disposeObject(root: Object3D) {
  root.traverse((obj) => {
    const mesh = obj as Mesh;
    mesh.geometry?.dispose();
    const materials: Material[] = Array.isArray(mesh.material) ? mesh.material : mesh.material ? [mesh.material] : [];
    for (const m of materials) {
      for (const value of Object.values(m)) {
        if (value && (value as Texture).isTexture) (value as Texture).dispose();
      }
      m.dispose();
    }
  });
}

export default function ModelPreview({ base64, extension }: ModelPreviewProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const resetRef = useRef<(() => void) | null>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "error" | "separate">("loading");
  const [error, setError] = useState("");

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    setStatus("loading");
    resetRef.current = null;

    const bytes = base64ToBytes(base64);
    const isText = extension === "gltf";
    const text = isText ? new TextDecoder().decode(bytes) : "";
    if (isText && usesSeparateFiles(text)) {
      setStatus("separate");
      return;
    }

    let renderer: WebGLRenderer;
    try {
      renderer = new WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
    } catch {
      setError("This computer can't show 3D previews right now.");
      setStatus("error");
      return;
    }
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    host.appendChild(renderer.domElement);
    renderer.domElement.style.display = "block";
    renderer.domElement.style.width = "100%";
    renderer.domElement.style.height = "100%";

    const scene = new Scene();
    scene.add(new HemisphereLight(0xffffff, 0x8a8a99, 1.6));
    const sun = new DirectionalLight(0xffffff, 2.2);
    sun.position.set(3, 5, 4);
    scene.add(sun);

    const camera = new PerspectiveCamera(45, 1, 0.01, 1000);
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;

    let model: Object3D | null = null;
    let frame = 0;
    let disposed = false;

    const size = () => {
      const w = Math.max(1, host.clientWidth);
      const h = Math.max(1, host.clientHeight);
      renderer.setSize(w, h, false);
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
    };
    size();
    const ro = new ResizeObserver(size);
    ro.observe(host);

    const frameModel = () => {
      if (!model) return;
      const box = new Box3().setFromObject(model);
      const center = box.getCenter(new Vector3());
      const radius = Math.max(box.getSize(new Vector3()).length() / 2, 0.001);
      const dist = radius / Math.sin((camera.fov * Math.PI) / 360);
      camera.near = dist / 100;
      camera.far = dist * 100;
      camera.position.copy(center).add(new Vector3(0.6, 0.45, 1).normalize().multiplyScalar(dist));
      camera.updateProjectionMatrix();
      controls.target.copy(center);
      controls.update();
    };
    resetRef.current = frameModel;

    const loader = new GLTFLoader();
    loader.parse(
      isText ? text : bytes.buffer,
      "",
      (gltf) => {
        if (disposed) {
          disposeObject(gltf.scene);
          return;
        }
        model = gltf.scene;
        scene.add(model);
        frameModel();
        setStatus("ready");
      },
      (err) => {
        if (disposed) return;
        setError(err instanceof Error ? err.message : "This model file couldn't be read.");
        setStatus("error");
      },
    );

    const tick = () => {
      frame = requestAnimationFrame(tick);
      controls.update();
      renderer.render(scene, camera);
    };
    tick();

    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      ro.disconnect();
      controls.dispose();
      if (model) disposeObject(model);
      renderer.dispose();
      renderer.domElement.remove();
      resetRef.current = null;
    };
  }, [base64, extension]);

  if (status === "separate") return <Note tone="info">{SEPARATE_FILES_NOTE}</Note>;

  return (
    <div className="flex flex-col gap-2">
      <div className="relative h-80 overflow-hidden rounded-lg border border-border bg-gradient-to-b from-muted/40 to-muted">
        <div ref={hostRef} className="absolute inset-0" data-testid="model-viewer" />
        {status === "loading" && (
          <p className="absolute inset-0 flex items-center justify-center text-sm text-muted-foreground">
            Loading the model…
          </p>
        )}
        {status === "error" && (
          <p className="absolute inset-0 flex items-center justify-center p-4 text-center text-sm text-muted-foreground">
            {error}
          </p>
        )}
        {status === "ready" && (
          <Button
            variant="outline"
            size="sm"
            className="absolute right-2 bottom-2 bg-popover/90"
            onClick={() => resetRef.current?.()}
          >
            <RotateCcw />
            Reset view
          </Button>
        )}
      </div>
    </div>
  );
}
