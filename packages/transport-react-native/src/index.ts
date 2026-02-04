import { ReactNativeTransport } from './ReactNativeTransport';

export { ReactNativeTransport };

export const createReactNativeTransport = (dbPath: string) =>
  new ReactNativeTransport(dbPath);
