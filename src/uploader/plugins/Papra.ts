import UploaderPlugin, {
  type configKeyType,
  type UploaderPluginInterface,
} from '../UploaderPlugin';

export default class PapraPlugin
  extends UploaderPlugin
  implements UploaderPluginInterface
{
  public override name = 'Papra';
  override configRequired: configKeyType = [
    'API_URL',
    'API_KEY',
    'ORGANIZATION_ID',
  ];
  override configOptional = ['OCR_LANGUAGE'];

  override get apiUrl(): string {
    const url = new URL(this.config.API_URL as string);
    const organizationId = this.config.ORGANIZATION_ID as string;
    url.pathname = `/api/organizations/${organizationId}/documents`;
    return url.toString();
  }

  override get headers(): Record<string, string> {
    return {
      'Content-Type': 'multipart/form-data',
      Authorization: `Bearer ${this.config.API_KEY as string}`,
    };
  }

  override createFormData(filePath: string): FormData {
    try {
      const fileContent = Bun.file(filePath);
      const formData = new FormData();

      formData.append('file', fileContent);
      if (this.config.OCR_LANGUAGE) {
        formData.append('ocr_language', this.config.OCR_LANGUAGE);
      }
      return formData;
    } catch (error) {
      throw new Error(
        `Failed to create FormData for file ${filePath}: ${error}`,
      );
    }
  }
}
