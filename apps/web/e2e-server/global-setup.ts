import { compose } from './stand';

export default function setup(): void {
  compose('up', '-d', '--wait', '--build');
}
