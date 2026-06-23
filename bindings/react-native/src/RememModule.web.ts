import { registerWebModule, NativeModule } from 'expo';

// RememModule is not available on the web platform.
// eslint-disable-next-line @typescript-eslint/no-empty-object-type -- see RememModule.ts
class RememModule extends NativeModule<{}> {}

export default registerWebModule(RememModule, 'RememModule');
