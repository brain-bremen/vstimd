// React hook: owns the Connection, keeps the latest SceneSnapshot as the read
// model, and exposes the Connection for issuing commands.

import { useEffect, useRef, useState } from "react";
import { Connection, type SceneSnapshot } from "../index.js";

const BASE_URL = `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}`;

export interface Scene {
  conn: Connection | null;
  snapshot: SceneSnapshot | null;
  connected: boolean;
}

export function useScene(): Scene {
  const [conn, setConn] = useState<Connection | null>(null);
  const [snapshot, setSnapshot] = useState<SceneSnapshot | null>(null);
  const [connected, setConnected] = useState(false);
  const connRef = useRef<Connection | null>(null);

  useEffect(() => {
    let closed = false;
    let sub: { close(): void } | undefined;

    (async () => {
      const c = await Connection.connect(BASE_URL);
      if (closed) {
        c.close();
        return;
      }
      connRef.current = c;
      setConn(c);
      setConnected(true);
      sub = await c.events(setSnapshot);
    })().catch((e) => console.error("scene connection failed", e));

    return () => {
      closed = true;
      sub?.close();
      connRef.current?.close();
    };
  }, []);

  return { conn, snapshot, connected };
}
