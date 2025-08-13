import { Spinner } from '@topcli/spinner';
import { $ } from 'bun';
import uploader from './uploader/uploader';

const dateFormat = process.env.date_format ?? '%Y-%m-%d-%H%M%S-%3N-%Z';
const filePrefix = process.env.file_prefix ?? 'scan';
const vendor = process.env.vendor ?? 'fujitsu';
const dpi = process.env.dpi ?? '300';
const mode = process.env.mode ?? 'Color';
const outputDir = process.env.output_dir ?? './scans';
const scanProgram = process.env.scan_program ?? '/app/sane-scan-pdf/scan';
const spinner = new Spinner();

// Run the scan command
try {
  const now = await $`date +${dateFormat}`.text();
  const fileOutputPath = `${outputDir}/${filePrefix}-${now}.pdf`;
  const flags = {
    '-v': '',
    '-d': '',
    '-x': vendor,
    '-r': dpi,
    '--mode': mode,
    '--skip-empty-pages': '',
    '--crop': '',
    '-o': fileOutputPath,
  };

  spinner.start(`Scanning document to ${fileOutputPath}`);
  await $`${scanProgram} ${Object.entries(flags).flat().filter(Boolean).join(' ')}`;
  spinner.succeed(`Scan completed: ${fileOutputPath}`);

  // upload the scanned document
  await uploader(fileOutputPath);
  process.exit(0);
} catch (error) {
  spinner.failed(`Scan failed: ${JSON.stringify(error)}`);
  process.exit(1);
}
