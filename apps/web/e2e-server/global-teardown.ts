import { compose } from './stand';

export default function teardown(): void {
  if (process.env.FERRUM_KEEP_STAND !== '1') compose('down', '-v');
}
