export interface OverwolfResult {
  success: boolean;
  error?: string;
}

export interface OverwolfEvent<T> {
  addListener(listener: (event: T) => void): void;
  removeListener(listener: (event: T) => void): void;
}

export interface OverwolfApi {
  games: {
    getRunningGameInfo(callback: (result: unknown) => void): void;
    events: {
      setRequiredFeatures(
        features: string[],
        callback: (result: OverwolfResult & { supportedFeatures?: string[] }) => void,
      ): void;
      getInfo(callback: (result: unknown) => void): void;
      onInfoUpdates2: OverwolfEvent<unknown>;
      onError: OverwolfEvent<unknown>;
    };
  };
  io: {
    writeFileContents(
      filePath: string,
      content: string,
      encoding: "UTF8",
      triggerUacIfRequired: false,
      callback: (result: OverwolfResult) => void,
    ): void;
  };
}

declare global {
  interface Window {
    overwolf?: OverwolfApi;
  }
}
