import './style.css';
import { mountOrb } from './orb';

const root = document.querySelector<HTMLElement>('#app');
if (!root) {
  throw new Error('Missing #app root');
}

mountOrb(root);
