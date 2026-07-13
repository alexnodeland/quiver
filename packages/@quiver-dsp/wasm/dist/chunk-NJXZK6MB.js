// src/audio.ts
async function createQuiverAudioNode(audioContext, options) {
  const { workletUrl, wasmUrl, outputChannels = 2 } = options;
  const wasmBytes = await fetch(String(wasmUrl)).then((r) => r.arrayBuffer());
  await audioContext.audioWorklet.addModule(String(workletUrl));
  const node = new AudioWorkletNode(audioContext, "quiver-processor", {
    numberOfInputs: 0,
    numberOfOutputs: 1,
    outputChannelCount: [outputChannels]
  });
  node.port.start();
  const RESPONSE_TIMEOUT_MS = 1e4;
  let nextRequestId = 1;
  const pending = /* @__PURE__ */ new Map();
  node.port.addEventListener("message", (event) => {
    const data = event.data;
    if (!data || typeof data.requestId !== "number") return;
    const entry = pending.get(data.requestId);
    if (!entry) {
      if (data.type === "error") {
        console.error("Quiver worklet error:", data.error);
      }
      return;
    }
    if (data.type === entry.okType) {
      clearTimeout(entry.timer);
      pending.delete(data.requestId);
      entry.resolve(entry.extract ? entry.extract(data) : void 0);
    } else if (data.type === "error") {
      clearTimeout(entry.timer);
      pending.delete(data.requestId);
      entry.reject(new Error(String(data.error)));
    }
  });
  const post = (message) => {
    node.port.postMessage({ ...message, requestId: nextRequestId++ });
  };
  await new Promise((resolve, reject) => {
    const timeout = setTimeout(
      () => reject(new Error("Quiver worklet initialization timeout")),
      RESPONSE_TIMEOUT_MS
    );
    const handler = (event) => {
      if (event.data?.type === "ready") {
        clearTimeout(timeout);
        node.port.removeEventListener("message", handler);
        resolve();
      } else if (event.data?.type === "error" && event.data?.requestId === void 0) {
        clearTimeout(timeout);
        node.port.removeEventListener("message", handler);
        reject(new Error(event.data.error));
      }
    };
    node.port.addEventListener("message", handler);
    node.port.postMessage(
      { type: "init", wasmBytes, sampleRate: audioContext.sampleRate },
      [wasmBytes]
    );
  });
  const awaitResponse = (message, okType, extract) => new Promise((resolve, reject) => {
    const requestId = nextRequestId++;
    const timer = setTimeout(() => {
      pending.delete(requestId);
      reject(
        new Error(
          `Quiver worklet request '${String(message.type)}' (#${requestId}) timed out`
        )
      );
    }, RESPONSE_TIMEOUT_MS);
    pending.set(requestId, {
      okType,
      resolve,
      reject,
      extract,
      timer
    });
    node.port.postMessage({ ...message, requestId });
  });
  return {
    node,
    context: audioContext,
    loadPatch: (patch) => awaitResponse({ type: "load_patch", patch }, "patch_loaded"),
    savePatch: (name) => awaitResponse({ type: "save_patch", name }, "patch_saved", (data) => data.patch),
    setParam: (nodeId, paramIndex, value) => post({ type: "set_param", nodeId, paramIndex, value }),
    addModule: (typeId, name) => post({ type: "add_module", typeId, name }),
    removeModule: (name) => post({ type: "remove_module", name }),
    connect: (from, to, attenuation, offset) => post({ type: "connect", from, to, attenuation, offset }),
    disconnect: (from, to) => post({ type: "disconnect", from, to }),
    setOutput: (name) => post({ type: "set_output", name }),
    addMidiInputs: () => post({ type: "add_midi_inputs" }),
    midiNoteOn: (note, velocity) => post({ type: "midi_note_on", note, velocity }),
    midiNoteOff: (note, velocity) => post({ type: "midi_note_off", note, velocity }),
    midiCc: (cc, value) => post({ type: "midi_cc", cc, value }),
    midiPitchBend: (value) => post({ type: "midi_pitch_bend", value }),
    compile: () => awaitResponse({ type: "compile" }, "compiled"),
    reset: () => post({ type: "reset" }),
    dispose: () => {
      post({ type: "destroy" });
      node.disconnect();
    }
  };
}
async function createQuiverAudio(options) {
  const audioContext = new AudioContext();
  const quiver = await createQuiverAudioNode(audioContext, options);
  quiver.node.connect(audioContext.destination);
  return quiver;
}

export {
  createQuiverAudioNode,
  createQuiverAudio
};
//# sourceMappingURL=chunk-NJXZK6MB.js.map