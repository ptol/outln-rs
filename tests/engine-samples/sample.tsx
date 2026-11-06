/**
 * Engine sample for TSX: a header comment, then components.
 */

'use client';

export function App(props: { name: string }) {
  return <div className="app">{props.name}</div>;
}

export const Button = ({ label }: { label: string }) => <button>{label}</button>;

export default class Page extends Component<Props> {
  render() {
    return <App name="page" />;
  }
}
