// A styled tooltip in place of the native `title`: it follows the theme, shows on
// keyboard focus as well as hover, and opens after a short rest. `<Button tooltip>` uses
// it; on any other element it is `use:tooltip={"text"}`.
//
// The tooltip is a hint for the eyes. The element still needs its own name
// (`aria-label` or text); when the tip says something else, it also becomes the
// element's accessible description.

const SHOW_AFTER = 450;
const GAP = 6;

export function tooltip(node: HTMLElement, text: string | undefined) {
  let tipEl: HTMLDivElement | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function describe() {
    if (text && text !== node.getAttribute("aria-label") && text !== node.textContent?.trim()) {
      node.setAttribute("aria-description", text);
    } else {
      node.removeAttribute("aria-description");
    }
  }

  function show() {
    clearTimeout(timer);
    if (!text || tipEl) return;
    tipEl = document.createElement("div");
    tipEl.className = "tooltip";
    tipEl.setAttribute("role", "tooltip");
    tipEl.popover = "manual";
    tipEl.textContent = text;
    document.body.append(tipEl);
    tipEl.showPopover();
    const at = node.getBoundingClientRect();
    const size = tipEl.getBoundingClientRect();
    const above = at.top - GAP - size.height >= 4;
    const top = above ? at.top - GAP - size.height : at.bottom + GAP;
    const left = Math.max(4, Math.min(at.left + at.width / 2 - size.width / 2, innerWidth - size.width - 4));
    tipEl.style.top = `${Math.round(top)}px`;
    tipEl.style.left = `${Math.round(left)}px`;
    tipEl.style.transformOrigin = above ? "bottom center" : "top center";
  }

  function hide() {
    clearTimeout(timer);
    tipEl?.remove();
    tipEl = null;
  }

  const onEnter = () => {
    clearTimeout(timer);
    timer = setTimeout(show, SHOW_AFTER);
  };
  const onFocus = () => {
    if (node.matches(":focus-visible")) show();
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") hide();
  };

  node.addEventListener("pointerenter", onEnter);
  node.addEventListener("pointerleave", hide);
  node.addEventListener("pointerdown", hide);
  node.addEventListener("focus", onFocus);
  node.addEventListener("blur", hide);
  node.addEventListener("keydown", onKey);
  addEventListener("scroll", hide, true);
  describe();

  return {
    update(next: string | undefined) {
      text = next;
      describe();
      if (tipEl) {
        if (text) tipEl.textContent = text;
        else hide();
      }
    },
    destroy() {
      hide();
      node.removeEventListener("pointerenter", onEnter);
      node.removeEventListener("pointerleave", hide);
      node.removeEventListener("pointerdown", hide);
      node.removeEventListener("focus", onFocus);
      node.removeEventListener("blur", hide);
      node.removeEventListener("keydown", onKey);
      removeEventListener("scroll", hide, true);
    },
  };
}
