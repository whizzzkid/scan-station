import uploaderPlugins from './plugins';
import type UploaderPlugin from './UploaderPlugin';

const activatedPlugins = uploaderPlugins.reduce((acc, plugin) => {
  try {
    const instance = new plugin();
    instance.init();
    acc.push(instance);
  } catch (_error) {
    // do not throw an error here;
  }
  return acc;
}, [] as UploaderPlugin[]);

export default async function uploader(filePath: string) {
  return Promise.all(
    activatedPlugins.map((plugin) => plugin.upload(filePath as string)),
  );
}
