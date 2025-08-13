import {
  expect,
  test,
  describe,
  beforeEach,
  afterEach,
  mock,
  beforeAll,
} from 'bun:test';
import UploaderPlugin, {
  type configKeyType,
  type UploaderPluginInterface,
} from './UploaderPlugin';

const mockSpinnerStart = mock();
const mockSpinnerSucceed = mock();
const mockSpinnerFailed = mock();

mock.module('@topcli/spinner', () => ({
  Spinner: function spinner() {
    return {
      start: mockSpinnerStart,
      succeed: mockSpinnerSucceed,
      failed: mockSpinnerFailed,
    };
  },
}));

const TEST_URL = 'http://test.com/api/upload';

/**
 * TestPlugin is a mock implementation of UploaderPlugin for testing purposes.
 * It simulates a plugin that uploads files to a test URL with specific headers.
 */
class TestPlugin extends UploaderPlugin implements UploaderPluginInterface {
  public override name = 'TestPlugin';
  override configRequired: configKeyType = ['TEST_URL', 'TEST_CONFIG_1'];
  override configOptional = ['TEST_CONFIG_OPT_1'];

  override get apiUrl(): string {
    return 'http://test.com/api/upload';
  }

  override get headers(): Record<string, string> {
    return {
      'Content-Type': 'multipart/form-data',
      Authorization: 'Bearer test-token',
      testConfig1: this.config.TEST_CONFIG_1 as string,
      testConfigOpt1: this.config.TEST_CONFIG_OPT_1 as string,
    };
  }

  override createFormData(filePath: string): FormData {
    const formData = new FormData();
    formData.append('filePath', Bun.file(filePath));
    formData.append('testField', 'testValue');
    return formData;
  }
}

describe('UploaderPlugin Unhappy Path: Missing Configs', () => {
  test('Should throw error if required config is missing', () => {
    const plugin = new TestPlugin();
    expect(() => plugin.init()).toThrowError();
  });
});

describe('Uploader Plugin Happy Path: Configs Exist in ENV.', () => {
  const fetchMock = mock();

  beforeAll(() => {
    globalThis.fetch = fetchMock as unknown as typeof fetch;
  });

  beforeEach(() => {
    process.env.TESTPLUGIN_TEST_URL = TEST_URL;
    process.env.TESTPLUGIN_TEST_CONFIG_1 = 'value2';
    process.env.TESTPLUGIN_TEST_CONFIG_OPT_1 = 'optionalValue1';
  });

  afterEach(() => {
    mock.clearAllMocks();
  });

  test('Should initialize the plugin', () => {
    const plugin = new TestPlugin();
    plugin.init();
    expect(plugin.name).toBe('TestPlugin');
    expect(plugin.configRequired).toEqual(['TEST_URL', 'TEST_CONFIG_1']);
    expect(plugin.configOptional).toEqual(['TEST_CONFIG_OPT_1']);
  });

  test('Should upload file correctly', async () => {
    const plugin = new TestPlugin();
    plugin.init();

    fetchMock.mockReturnValue(
      Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ success: true }),
      }),
    );

    await plugin.upload('test-file.pdf');
    expect(fetchMock).toHaveBeenCalledWith(
      TEST_URL,
      expect.objectContaining({
        method: 'POST',
        headers: {
          'Content-Type': 'multipart/form-data',
          Authorization: 'Bearer test-token',
          testConfig1: 'value2',
          testConfigOpt1: 'optionalValue1',
        },
      }),
    );
  });

  test('Should handle upload failure', async () => {
    const plugin = new TestPlugin();
    plugin.init();

    fetchMock.mockReturnValue(
      Promise.resolve({
        ok: false,
        status: 500,
        text: () => Promise.resolve('Internal Server Error'),
      }),
    );

    await expect(plugin.upload('test-file.pdf')).rejects;
    expect(mockSpinnerFailed).toHaveBeenCalledWith(
      `Failed to upload test-file.pdf to TestPlugin: Error: HTTP error! status: 500`,
    );
  });
});
