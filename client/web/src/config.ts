// Named scene-config persistence. Mirrors vstimd.config (Python).
//
// The server keeps a config directory (set via --config-dir); each config is a
// `<name>.config.json` file. Names are bare — no path, no extension. `retrieve`
// returns the current scene + I/O config as a JSON string (the same format as
// the on-disk files), which `upload` accepts back.

import { create, type MessageInitShape } from "@bufbuild/protobuf";
import { RequestSchema } from "./_proto/vstimd/v1/service_pb.js";
import type { Send } from "./transport.js";

/** Options for {@link ConfigClient.upload}. */
export interface UploadOpts {
  /** Replace an existing config with the same name (default: error if it exists). */
  overwrite?: boolean;
  /** Apply the config immediately after saving. */
  applyNow?: boolean;
  /** Only when `applyNow`: merge into the scene instead of replacing it. */
  additive?: boolean;
}

export class ConfigClient {
  constructor(private readonly send: Send) {}

  /** Bare config names available in the server's config directory. */
  async list(): Promise<string[]> {
    const resp = await this.system({ case: "listConfigs", value: {} });
    return resp.body.case === "configList" ? resp.body.value.names : [];
  }

  /**
   * Load a named config. With `additive`, merge stimuli/animations into the
   * current scene (handles remapped); otherwise the scene is cleared first.
   * The I/O config (VTL names) is always fully replaced.
   */
  async load(name: string, opts: { additive?: boolean } = {}): Promise<void> {
    await this.system({ case: "loadConfig", value: { name, additive: opts.additive ?? false } });
  }

  /** Return the current scene + I/O config as a JSON string. */
  async retrieve(): Promise<string> {
    const resp = await this.system({ case: "retrieveConfig", value: {} });
    return resp.body.case === "retrievedConfig" ? resp.body.value.json : "";
  }

  /** Upload a config JSON string (as produced by {@link retrieve}) under `name`. */
  async upload(name: string, json: string, opts: UploadOpts = {}): Promise<void> {
    await this.system({
      case: "uploadConfig",
      value: {
        name,
        json,
        overwrite: opts.overwrite ?? false,
        applyNow: opts.applyNow ?? false,
        additive: opts.additive ?? false,
      },
    });
  }

  /** Retrieve the current scene and save it under `name` in one call. */
  async save(name: string, opts: { overwrite?: boolean } = {}): Promise<void> {
    const json = await this.retrieve();
    await this.upload(name, json, { overwrite: opts.overwrite });
  }

  private system(body: MessageInitShape<typeof RequestSchema>["body"]) {
    return this.send(create(RequestSchema, { target: { case: "system", value: {} }, body }));
  }
}
