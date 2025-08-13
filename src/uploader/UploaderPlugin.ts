import { Spinner } from '@topcli/spinner';

export type configKeyType = string[];
type configObjectType = Record<configKeyType[number], string>;
export interface UploaderPluginInterface {
  name: string;
  upload(file: string): Promise<void>;
}

/**
 * Base class for uploader plugins.
 * It provides methods to load configuration from environment variables,
 */
export default class UploaderPlugin implements UploaderPluginInterface {
  private readonly spinner = new Spinner();
  protected config!: configObjectType;
  protected readonly configOptional!: configKeyType;
  protected readonly configRequired!: configKeyType;
  public readonly name!: string;

  /**
   * Initializes the plugin by loading the configuration.
   */
  public init() {
    try {
      this.config = this.loadConfig();
      // don't log values in console as they may contain sensitive information.
      this.spinner.start(`Plugin ${this.name} initialized with config`);
    } catch (error) {
      this.spinner.failed(`Failed to initialize plugin ${this.name}: ${error}`);
      throw new Error(
        `Failed to load config for plugin ${this.name}: ${error}.`,
      );
    }
  }

  private loadConfig(): configObjectType {
    if (!this.name) {
      throw new Error('Plugin name is not defined');
    }

    const requiredConfig = this.configRequired.reduce(
      (acc, key): configObjectType => {
        const envKey = this.envKey(key);
        if (!(envKey in process.env)) {
          throw new Error(
            `Environment variable ${envKey} is required for plugin ${this.name}`,
          );
        }
        acc[key] = process.env[envKey] as string;
        return acc;
      },
      {} as configObjectType,
    );

    const optionalConfig = this.configOptional.reduce((acc, key) => {
      const envKey = this.envKey(key);
      if (envKey in process.env) {
        acc[key] = process.env[envKey] as string;
      }
      return acc;
    }, {} as configObjectType);

    return { ...optionalConfig, ...requiredConfig };
  }

  /**
   * Uploads a file to the configured API endpoint.
   *
   * @param file The filepath to upload.
   */
  public async upload(file: string): Promise<void> {
    this.spinner.start(`Uploading ${file} to ${this.name}`);
    try {
      const response = await fetch(this.apiUrl, {
        method: 'POST',
        headers: this.headers,
        body: this.createFormData(file),
      });
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      this.spinner.succeed(
        `Successfully uploaded ${file} to ${this.name}: ${JSON.stringify(response)}`,
      );
    } catch (error) {
      this.spinner.failed(`Failed to upload ${file} to ${this.name}: ${error}`);
    }
  }

  private envKey(key: string): string {
    return `${this.name.toUpperCase()}_${key.toUpperCase()}`;
  }

  protected createFormData(_filePath: string): FormData {
    throw new Error(
      `createFormData method not implemented for plugin ${this.name}`,
    );
  }

  protected get apiUrl(): string {
    throw new Error(`apiUrl getter not implemented for plugin ${this.name}`);
  }

  protected get headers(): Record<string, string> {
    throw new Error(`headers getter not implemented for plugin ${this.name}`);
  }
}
