// Context menus and toast notifications.
import { el, esc } from './util.js';
import { icon } from './icons.js';

let current = null;

export function closeMenu() {
  if (!current) return;
  current.remove();
  current = null;
  document.removeEventListener('mousedown', outside, true);
  window.removeEventListener('blur', closeMenu);
}

function outside(e) {
  if (current && !current.contains(e.target)) closeMenu();
}

export const isMenuOpen = () => !!current;

/** items: {label, icon?, hint?, action?, disabled?, checked?} or '-' for a separator */
export function showMenu(x, y, items) {
  closeMenu();
  const menu = el('div', { class: 'menu', role: 'menu' });
  for (const it of items) {
    if (it === '-') {
      menu.append(el('div', { class: 'menu-sep' }));
      continue;
    }
    if (!it) continue;
    const b = el('button', {
      class: `menu-item${it.checked ? ' checked' : ''}${it.danger ? ' danger' : ''}`,
      role: 'menuitem',
      disabled: it.disabled,
      html: `${it.icon ? icon(it.icon) : '<span class="ico"></span>'}<span class="menu-label">${esc(it.label)}</span>${it.hint ? `<span class="menu-hint">${esc(it.hint)}</span>` : ''}`,
    });
    b.addEventListener('click', () => {
      closeMenu();
      it.action?.();
    });
    menu.append(b);
  }
  document.body.append(menu);
  const r = menu.getBoundingClientRect();
  menu.style.left = `${Math.max(4, Math.min(x, innerWidth - r.width - 6))}px`;
  menu.style.top = `${Math.max(4, Math.min(y, innerHeight - r.height - 6))}px`;
  current = menu;
  menu.querySelector('.menu-item:not([disabled])')?.focus({ preventScroll: true });
  setTimeout(() => {
    document.addEventListener('mousedown', outside, true);
    window.addEventListener('blur', closeMenu);
  });
  menu.addEventListener('keydown', (e) => {
    const btns = [...menu.querySelectorAll('.menu-item:not([disabled])')];
    const i = btns.indexOf(document.activeElement);
    if (e.key === 'ArrowDown') btns[(i + 1) % btns.length]?.focus();
    else if (e.key === 'ArrowUp') btns[(i - 1 + btns.length) % btns.length]?.focus();
    else if (e.key === 'Escape') closeMenu();
    else return;
    e.preventDefault();
    e.stopPropagation();
  });
}

/**
 * Modal confirmation. The confirm button has focus, so Enter/Space confirm and
 * Esc cancels. Resolves to true when confirmed.
 */
export function confirmDialog({ title, body, confirmLabel = 'Tamam', cancelLabel = 'İptal', danger = false }) {
  return new Promise((resolve) => {
    const ok = el('button', { class: `btn${danger ? ' danger' : ' primary'}`, text: confirmLabel });
    const cancel = el('button', { class: 'btn', text: cancelLabel });
    const dlg = el('dialog', { class: 'confirm' }, el('h2', { text: title }), el('div', { class: 'confirm-body' }, body), el('footer', { class: 'confirm-foot' }, cancel, ok));
    let result = false;
    ok.addEventListener('click', () => {
      result = true;
      dlg.close();
    });
    cancel.addEventListener('click', () => dlg.close());
    dlg.addEventListener('keydown', (e) => {
      e.stopPropagation();
      // Enter confirms (or cancels when the cancel button has focus) without relying on keypress.
      if (e.key === 'Enter') {
        e.preventDefault();
        (document.activeElement === cancel ? cancel : ok).click();
      }
    });
    dlg.addEventListener('close', () => {
      dlg.remove();
      resolve(result);
    });
    document.body.append(dlg);
    dlg.showModal();
    ok.focus();
  });
}

export function toast(message, kind = 'info', ms = 3200) {
  const host = document.getElementById('toasts');
  const t = el('div', { class: `toast ${kind}`, role: 'status', text: message });
  host.append(t);
  requestAnimationFrame(() => t.classList.add('in'));
  setTimeout(() => {
    t.classList.remove('in');
    setTimeout(() => t.remove(), 250);
  }, ms);
}
