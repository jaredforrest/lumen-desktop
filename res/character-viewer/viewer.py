"""Compact character viewer for Sway. Unicode data is bundled; no network at runtime."""
import ctypes
import ctypes.util
import fcntl
import json
import os
from pathlib import Path
import re
import socket
import sys
import subprocess as sp
import unicodedata

HERE = Path(__file__).resolve().parent
STATE = Path(os.environ.get('XDG_STATE_HOME', Path.home() / '.local/state')) / 'character-viewer/state.json'
TONES = range(0x1F3FB, 0x1F400)
GROUPS = [
    ('Recent', '◷'), ('Favorites', '☆'), ('Smileys & People', '☺'),
    ('Animals & Nature', '♧'), ('Food & Drink', '♨'), ('Activities', '⚽'),
    ('Travel & Places', '✈'), ('Objects', '♧'), ('Symbols', '⌘'), ('Flags', '⚑'),
]
CHARACTER_GROUPS = ['Arrows', 'Currency Symbols', 'Latin', 'Letterlike Symbols', 'Maths Symbols', 'Punctuation']
GROUP_MAP = {'Smileys & Emotion': 'Smileys & People', 'People & Body': 'Smileys & People'}


def base_key(text):
    key = ''.join(c for c in text if ord(c) not in TONES and c != '\ufe0f')
    # Unicode encodes mixed-tone handshakes as a separate joined pair.
    return '🤝' if key == '🫱\u200d🫲' else key


def load_data():
    records, variants = [], {}
    group = ''
    for line in (HERE / 'emoji-test.txt').read_text().splitlines():
        if line.startswith('# group: '):
            group = GROUP_MAP.get(line[9:], line[9:])
        match = re.match(r'^([0-9A-F ]+)\s*; fully-qualified\s*# .+? E[\d.]+ (.+)$', line)
        if not match or group == 'Component':
            continue
        text = ''.join(chr(int(cp, 16)) for cp in match[1].split())
        item = {'text': text, 'name': match[2], 'group': group}
        variants.setdefault(base_key(text), []).append(item)
        if not any(ord(c) in TONES for c in text):
            records.append(item)
    # Named character blocks are available in the expanded viewer as well as search.
    blocks = {
        'Arrows': [(0x2190, 0x2200)], 'Currency Symbols': [(0x20a0, 0x20c1)],
        'Latin': [(0x00c0, 0x0250)], 'Letterlike Symbols': [(0x2100, 0x2150)],
        'Maths Symbols': [(0x2200, 0x2300)], 'Punctuation': [(0x2000, 0x2070)],
    }
    added = set()
    def symbol(text, section):
        if text in added or unicodedata.category(text)[0] in ('C', 'Z'):
            return
        added.add(text)
        records.append({'text': text, 'name': unicodedata.name(text, text).lower(),
                        'group': 'Symbols', 'section': section, 'symbol': True})
    for section, ranges in blocks.items():
        for start, end in ranges:
            for cp in range(start, end):
                symbol(chr(cp), section)
    for text in '⌘⌥⌃⇧⇪⎋⇥↩⌫⌦⏎©®™°±×÷πµ∆€£¥¢₩₹₽₿':
        symbol(text, 'Currency Symbols' if unicodedata.category(text) == 'Sc' else 'Letterlike Symbols')
    return records, variants


def sway(*args):
    return json.loads(sp.check_output(['swaymsg', '-r', *args], text=True))


def focused(node):
    if node.get('focused') and node.get('type') == 'con':
        return node
    for child in node.get('nodes', []) + node.get('floating_nodes', []):
        found = focused(child)
        if found:
            return found
    return None


def picker_workspace(node, workspace=None):
    if node.get('type') == 'workspace':
        workspace = node['rect']
    if node.get('app_id') == 'lumen-character-viewer':
        return node, workspace
    for child in node.get('nodes', []) + node.get('floating_nodes', []):
        result = picker_workspace(child, workspace)
        if result:
            return result
    return None


def load_state():
    try:
        data = json.loads(STATE.read_text())
        if not isinstance(data, dict):
            return {}
        return {k: v for k, v in data.items() if
                (k in ('recent', 'favorites') and isinstance(v, list) and all(isinstance(x, str) for x in v))
                or (k == 'expanded' and isinstance(v, bool))
                or (k == 'tones' and isinstance(v, dict) and all(isinstance(x, str) and isinstance(y, str) for x, y in v.items()))}
    except (OSError, ValueError):
        return {}


def save_state(data):
    STATE.parent.mkdir(parents=True, exist_ok=True)
    temporary = STATE.with_suffix('.tmp')
    STATE.parent.chmod(0o700)
    with temporary.open('w') as handle:
        os.chmod(temporary, 0o600)
        json.dump(data, handle, ensure_ascii=False)
        handle.flush()
        os.fsync(handle.fileno())
    temporary.replace(STATE)


def ipc_path():
    name = re.sub(r'[^A-Za-z0-9_-]', '_', os.environ.get('WAYLAND_DISPLAY', 'wayland'))
    return Path(os.environ['XDG_RUNTIME_DIR']) / f'lumen-character-viewer-{name}.sock'


def request_daemon(command):
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(1)
            client.connect(str(ipc_path()))
            client.sendall(command.encode())
            reply = client.recv(1024).decode()
            if command == 'status':
                print(reply)
            return bool(reply)
    except (OSError, TimeoutError):
        return False


def main():
    daemon = '--daemon' in sys.argv
    if not daemon and '--standalone' not in sys.argv:
        if request_daemon('status' if '--status' in sys.argv else 'show'):
            return
        if '--status' in sys.argv:
            print('{"running": false}')
            return
    # A second shortcut invocation never creates another window or steals the target.
    lock = open(Path(os.environ['XDG_RUNTIME_DIR']) / ('lumen-character-input.lock' if daemon else 'lumen-character-viewer.lock'), 'w')
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        return
    target = focused(sway('-t', 'get_tree'))
    # Register this font only in the picker, without changing the desktop font setup.
    if os.environ.get('CHARACTER_VIEWER_FONT'):
        fc = ctypes.CDLL(os.environ['CHARACTER_VIEWER_FONTCONFIG'])
        fc.FcConfigGetCurrent.restype = ctypes.c_void_p
        fc.FcConfigAppFontAddFile.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        fc.FcConfigAppFontAddFile(fc.FcConfigGetCurrent(), os.fsencode(os.environ['CHARACTER_VIEWER_FONT']))
    # Apply compositor decoration and geometry before the first mapped frame.
    for command in ('floating enable', 'border none',
                    'resize set width 352 px height 402 px', 'move position mouse', 'opacity 0'):
        sway(f'for_window [app_id="lumen-character-viewer"] {command}')
    import gi
    gi.require_version('Gtk', '3.0')
    gi.require_version('Gdk', '3.0')
    from gi.repository import Gtk, Gdk, GLib, Pango
    GLib.set_prgname('lumen-character-viewer')
    GLib.set_application_name('Emoji & Symbols')
    Gtk.IconTheme.get_default().append_search_path(str(HERE / 'icons'))
    records, variants = load_data()
    lookup = {i['text']: i for group in variants.values() for i in group}
    lookup.update({i['text']: i for i in records})
    state = load_state()
    current_window = None
    bridge = None
    native = False
    generation = 0
    css = Gtk.CssProvider()
    css_template = '''
        window { background: transparent; }
        #panel { background: BACKGROUND; border: 1px solid BORDER; border-radius: 16px; color: FOREGROUND; }
        #panel * { font-family: "FONT", sans-serif; font-size: 13px; }
        #panel button { background: transparent; border: 0; box-shadow: none; text-shadow: none; border-radius: 6px; padding: 0; min-height: 0; min-width: 0; color: FOREGROUND; }
        #panel button:hover { background: HOVER; }
        #panel button:checked { background: HOVER; }
        #panel button:focus { box-shadow: inset 0 0 0 1px ACCENT; }
        #panel .emoji, #panel .character { min-width: 36px; min-height: 37px; }
        #panel .character label { font-family: "FONT", sans-serif; font-size: 23px; }
        #panel .emoji label { font-family: "EMOJI_FAMILY", "Noto Color Emoji"; font-size: 25px; }
        #panel entry { background: SEARCH; color: FOREGROUND; border: 1px solid BORDER; border-radius: 7px; box-shadow: none; min-height: 28px; padding: 0 8px; }
        #panel entry:focus { border-color: ACCENT; }
        #panel .heading { color: MUTED; font-size: 12px; font-weight: 600; }
        #panel .toolbar { border-top: 1px solid BORDER; padding: 7px 6px; }
        #panel .category { min-width: 29px; min-height: 26px; }
        #panel .category label { font-family: sans-serif; font-size: 19px; }
        #panel .sidebar { background: SEARCH; border-right: 1px solid BORDER; padding: 10px; }
        #panel .sidebar button { padding: 7px 8px; }
        #panel .sidebar button:checked { background: ACCENT; color: white; }
        #panel .details { border-left: 1px solid BORDER; padding: 12px; }
        #panel .preview { font-family: "EMOJI_FAMILY", sans-serif; font-size: 64px; }
        #panel .text-preview { font-family: "FONT", sans-serif; font-size: 56px; }
        #panel .detail-name { font-weight: 600; }
        #panel .action { background: SEARCH; border: 1px solid BORDER; padding: 5px 8px; }
        #panel .emoji.selected { background: HOVER; box-shadow: inset 0 0 0 1px ACCENT; }
        #character-options { background: BACKGROUND; color: FOREGROUND; border: 1px solid BORDER; border-radius: 10px; }
        #character-options * { font-family: "FONT", sans-serif; font-size: 12px; }
        #character-options button { background: transparent; color: FOREGROUND; border: 0; box-shadow: none; padding: 5px; border-radius: 5px; }
        #character-options button:hover, #character-options button:focus { background: HOVER; }
        #character-options .emoji label { font-family: "EMOJI_FAMILY", "Noto Color Emoji"; font-size: 25px; }
        #character-options .emoji { min-width: 32px; min-height: 34px; padding: 0; }
        tooltip { background: BACKGROUND; color: FOREGROUND; border: 1px solid BORDER; border-radius: 6px; }
        tooltip label { font-family: "FONT", sans-serif; font-size: 11px; }
        #panel .hint { color: MUTED; font-size: 11px; }
        #panel flowboxchild { padding: 0; margin: 0; }
        #panel scrollbar slider { min-width: 4px; background: MUTED; border: 0; }
        #panel scrollbar { background: transparent; }
    '''
    appearance_path = Path(os.environ.get('LUMEN_APPEARANCE_STATE',
        str(Path(os.environ.get('XDG_STATE_HOME', Path.home() / '.local/state')) / 'lumen-desktop/appearance.json')))
    last_appearance = None
    def update_appearance():
        nonlocal last_appearance
        try:
            appearance = json.loads(appearance_path.read_text())
        except (OSError, ValueError):
            palette_path = HERE / 'palette.json'
            if not palette_path.exists():
                palette_path = HERE.parent / 'lumen-desktop/appearance/palette.json'
            palettes = json.loads(palette_path.read_text())
            appearance = {'palette': palettes['light'], 'font': palettes['font']}
        if appearance != last_appearance:
            colors = appearance['palette']
            rendered = css_template
            for token, key in {'BACKGROUND':'background','FOREGROUND':'text','BORDER':'border',
                               'HOVER':'hover','SEARCH':'field','MUTED':'muted','ACCENT':'accent'}.items():
                rendered = rendered.replace(token, colors[key])
            rendered = rendered.replace('FONT', appearance['font'])
            family = os.environ.get('LUMEN_EMOJI_FONT_FAMILY', 'Noto Color Emoji').replace('\\', '\\\\').replace(chr(34), '\\'+chr(34))
            rendered = rendered.replace('EMOJI_FAMILY', family)
            css.load_from_data(rendered.encode())
            Gtk.Settings.get_default().set_property('gtk-theme-name', 'WhiteSur-Dark' if appearance.get('mode') == 'dark' else 'WhiteSur-Light')
            Gtk.Settings.get_default().set_property('gtk-application-prefer-dark-theme', appearance.get('mode') == 'dark')
            last_appearance = appearance
        return True
    update_appearance()
    GLib.timeout_add_seconds(1, update_appearance)
    Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)

    class Viewer(Gtk.Window):
        def __init__(self):
            super().__init__(title='Emoji & Symbols')
            self.set_decorated(False)
            self.set_resizable(False)
            self.set_default_size(352, 402)
            self.set_type_hint(Gdk.WindowTypeHint.DIALOG)
            visual = self.get_screen().get_rgba_visual()
            if visual:
                self.set_visual(visual)
            self.expanded = state.get('expanded', False)
            self.selected = None
            self.selected_button = None
            self.button_items = {}
            self.choosing = False
            self.popover = None
            self.section = 'Recent'
            self.pending = 0
            self.native = native
            self.destroying = False
            self.position_check = 0
            self.generation = generation
            self.target = target
            if self.native:
                self.connect('realize', self.attach_input_surface)
            else:
                self.connect('map-event', self.prepare_position)
            self.connect('destroy', self.closed)
            self.connect('key-press-event', self.key)
            self.connect('focus-out-event', self.focus_out)
            self.panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
            self.panel.set_name('panel')
            self.add(self.panel)
            header = Gtk.Box(spacing=8, margin=12)
            self.search = Gtk.SearchEntry(placeholder_text='Search')
            self.search.connect('search-changed', self.search_changed)
            self.search.connect('activate', self.enter)
            header.pack_start(self.search, True, True, 0)
            expand = Gtk.Button.new_from_icon_name('character-viewer-symbolic', Gtk.IconSize.MENU)
            expand.set_tooltip_text('Expand / collapse Character Viewer')
            expand.get_accessible().set_name('Expand / collapse Character Viewer')
            expand.connect('clicked', self.expand)
            header.pack_end(expand, False, False, 0)
            self.panel.pack_start(header, False, False, 0)
            content = Gtk.Box()
            self.sidebar = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
            self.sidebar.get_style_context().add_class('sidebar')
            self.sidebar_buttons = {}
            for name, icon in GROUPS + [(n, '') for n in CHARACTER_GROUPS]:
                btn = Gtk.Button(label=name.replace('Favorites', 'Favourites'), halign=Gtk.Align.FILL)
                btn.get_child().set_xalign(0)
                btn.connect('clicked', lambda b, n=name: self.select_section(n))
                self.sidebar.pack_start(btn, False, False, 0)
                self.sidebar_buttons[name] = btn
            self.sidebar_scroll = Gtk.ScrolledWindow()
            self.sidebar_scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
            self.sidebar_scroll.set_size_request(166, -1)
            self.sidebar_scroll.add(self.sidebar)
            content.pack_start(self.sidebar_scroll, False, False, 0)
            grid_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6, margin_start=10, margin_end=10)
            self.scroll = Gtk.ScrolledWindow()
            self.scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
            self.scroll.set_size_request(330, 272)
            self.sections = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
            self.sections.set_valign(Gtk.Align.START)
            self.scroll.add(self.sections)
            grid_box.pack_start(self.scroll, True, True, 0)
            self.empty = Gtk.Label(label='No characters found', margin=24)
            grid_box.pack_start(self.empty, False, False, 0)
            content.pack_start(grid_box, True, True, 0)
            self.details = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
            self.details.get_style_context().add_class('details')
            self.details.set_size_request(170, -1)
            self.preview = Gtk.Label(label='')
            self.preview.get_style_context().add_class('preview')
            self.details.pack_start(self.preview, False, False, 4)
            self.detail_name = Gtk.Label(label='Select a character', wrap=True, justify=Gtk.Justification.CENTER)
            self.detail_name.set_max_width_chars(20)
            self.detail_name.get_style_context().add_class('detail-name')
            self.details.pack_start(self.detail_name, False, False, 0)
            self.detail_code = Gtk.Label(label='', wrap=True, selectable=True)
            self.detail_code.set_max_width_chars(22)
            self.detail_code.get_style_context().add_class('hint')
            self.details.pack_start(self.detail_code, False, False, 0)
            self.detail_actions = []
            for label, action in [('Insert', lambda: self.choose(self.selected)),
                                  ('Copy', lambda: self.choose(self.selected, copy_only=True)),
                                  ('Add to Favourites', lambda: self.favorite(None, self.selected))]:
                btn = Gtk.Button(label=label)
                btn.get_style_context().add_class('action')
                btn.connect('clicked', lambda b, fn=action: fn() if self.selected else None)
                self.details.pack_start(btn, False, False, 0)
                self.detail_actions.append(btn)
            self.detail_favorite = self.detail_actions[-1]
            content.pack_start(self.details, False, False, 0)
            self.panel.pack_start(content, True, True, 0)
            toolbar = Gtk.Box(homogeneous=True)
            toolbar.get_style_context().add_class('toolbar')
            self.categories = {}
            for name, icon in GROUPS:
                button = Gtk.ToggleButton()
                names = ['emoji-recent', 'starred', 'emoji-people', 'emoji-nature', 'emoji-food', 'emoji-activities', 'emoji-travel', 'emoji-objects', 'emoji-symbols', 'emoji-flags']
                button.add(Gtk.Image.new_from_icon_name(names[[n for n, _ in GROUPS].index(name)] + '-symbolic', Gtk.IconSize.MENU))
                button.get_style_context().add_class('category')
                button.set_tooltip_text(name)
                button.get_accessible().set_name(name)
                button.connect('clicked', lambda b, n=name: self.select_section(n))
                toolbar.pack_start(button, True, True, 0)
                self.categories[name] = button
            self.panel.pack_start(toolbar, False, False, 0)
            self.sidebar_scroll.show_all()
            self.details.show_all()
            self.sidebar_scroll.set_no_show_all(True)
            self.details.set_no_show_all(True)
            self.empty.set_no_show_all(True)
            self.render()
            if self.native:
                self.panel.set_opacity(0)
            self.sidebar_scroll.set_visible(self.expanded)
            self.details.set_visible(self.expanded)
            if self.expanded:
                self.sidebar.show_all()
                self.details.show_all()
            self.set_size_request(*self.dimensions())
            self.resize(*self.dimensions())
            self.show_all()
            self.search.grab_focus()
            if self.native:
                event = Gdk.Event.new(Gdk.EventType.FOCUS_CHANGE)
                event.window = self.get_window()
                event.in_ = True
                self.event(event)
                self.position_check = GLib.timeout_add(120, self.verify_caret)

        def verify_caret(self):
            nonlocal native, current_window
            self.position_check = 0
            if self.destroying:
                return False
            if bridge.cv_positioned():
                self.panel.set_opacity(1)
            else:
                # Some apps enable text-input but never publish a cursor rectangle.
                # Sway anchors those at the entire window; use our visible fallback.
                self.destroy()
                native = False
                current_window = Viewer()
            return False

        def destroy(self):
            if self.destroying:
                return
            self.destroying = True
            if self.position_check:
                GLib.source_remove(self.position_check)
                self.position_check = 0
            if self.native:
                bridge.cv_hide()
            super().destroy()

        def do_unrealize(self):
            # Destroy the protocol role before GTK destroys its underlying wl_surface.
            if getattr(self, 'native', False):
                bridge.cv_hide()
            Gtk.Window.do_unrealize(self)

        def attach_input_surface(self, *_):
            # PyGObject's hash is the underlying GObject pointer.
            if not bridge.cv_attach(ctypes.c_void_p(hash(self))):
                GLib.idle_add(self.destroy)

        def closed(self, *_):
            nonlocal current_window
            if self.pending:
                GLib.source_remove(self.pending)
                self.pending = 0
            if self.native:
                bridge.cv_hide()
            if current_window is self:
                current_window = None
            if not daemon:
                Gtk.main_quit()

        def dimensions(self):
            # Use the focused output's usable area, including the bar/Dock reserve.
            try:
                outputs = sway('-t', 'get_workspaces')
                area = next(w['rect'] for w in outputs if w.get('focused'))
                available_width = area['width']
                if self.native and self.target:
                    available_width = min(available_width, max(352, self.target['rect']['width']-24))
                wide = available_width >= 680
            except (OSError, sp.SubprocessError, StopIteration, KeyError):
                area, wide, available_width = {'height': 1000}, True, 740
            self.details.set_visible(self.expanded and wide)
            self.sidebar_scroll.set_visible(self.expanded and available_width >= 540)
            return (min(740, available_width) if wide else min(540, available_width), min(490, max(380, area['height']))) if self.expanded else (352, min(402, max(380, area['height'])))

        def position(self):
            self.set_size_request(*self.dimensions())
            if self.native:
                self.resize(*self.dimensions())
                return False
            width, height = self.dimensions()
            sway(f'[app_id="lumen-character-viewer"] floating enable, border none, resize set width {width} px height {height} px')
            GLib.idle_add(self.reveal_position)
            # Repositioning can transfer focus with Sway focus-follows-mouse.
            sway('[app_id="lumen-character-viewer"] focus')
            return False

        def prepare_position(self, *_):
            # Sway maps invisibly; reveal only after clamping to its usable area.
            if not self.position_check:
                self.position_check = GLib.timeout_add(10, self.reveal_position)

        def reveal_position(self):
            if self.destroying:
                self.position_check = 0
                return False
            result = picker_workspace(sway('-t', 'get_tree'))
            if not result:
                return True
            node, area = result
            if area:
                desired = self.dimensions()
                if abs(node['rect']['width']-desired[0]) > 2 or abs(node['rect']['height']-desired[1]) > 2:
                    sway(f'[con_id={node["id"]}] resize set width {desired[0]} px height {desired[1]} px')
                    return True
                rect = node['rect']
                x = max(area['x'], min(rect['x'], area['x'] + area['width'] - rect['width']))
                y = max(area['y'], min(rect['y'], area['y'] + area['height'] - rect['height']))
                sway(f'[con_id={node["id"]}] move absolute position {x} px {y} px, opacity 1, focus')
            else:
                sway(f'[con_id={node["id"]}] opacity 1, focus')
            self.position_check = 0
            return False

        def focus_out(self, *_):
            if not self.native and not self.choosing and not (self.popover and self.popover.get_visible()):
                GLib.timeout_add(150, self.dismiss_if_unfocused)
            return False

        def dismiss_if_unfocused(self):
            if not self.is_active() and not self.choosing and not (self.popover and self.popover.get_visible()):
                self.destroy()
            return False

        def search_changed(self, *_):
            if self.pending:
                GLib.source_remove(self.pending)
            self.pending = GLib.timeout_add(70, self.render)

        def select_section(self, name):
            self.section = name
            self.search.set_text('')
            self.render()

        def expand(self, *_):
            self.expanded = not self.expanded
            state['expanded'] = self.expanded
            save_state(state)
            self.details.set_visible(self.expanded)
            if self.expanded:
                self.details.show_all()
            self.sidebar_scroll.set_visible(self.expanded)
            if self.expanded:
                self.sidebar.show_all()
            self.render()
            self.position()

        def render(self):
            if self.pending:
                GLib.source_remove(self.pending)
                self.pending = 0
            query = self.search.get_text().strip().lower()
            sections = []
            if query:
                words = query.replace('_', ' ').replace('-', ' ').split()
                def matches(item):
                    codes = ' '.join(f'{ord(c):04x}' for c in item['text'])
                    haystack = (item['name'] + ' ' + item['text'] + ' ' + codes).lower().replace('-', ' ')
                    return all(w.removeprefix('u+') in haystack for w in words)
                sections = [('Search Results', [i for i in records if matches(i)])]
            elif self.section == 'Recent':
                recent = [lookup[t] for t in state.get('recent', []) if t in lookup]
                if recent:
                    sections.append(('Recently Used', recent if self.expanded else recent[:16]))
                sections.append(('Smileys & People', [i for i in records if i['group'] == 'Smileys & People']))
            elif self.section == 'Favorites':
                sections = [('Favourites', [lookup[t] for t in state.get('favorites', []) if t in lookup])]
            else:
                sections = [(self.section, [i for i in records if i['group'] == self.section or i.get('section') == self.section])]
            for mapping in (self.categories, self.sidebar_buttons):
                for name, button in mapping.items():
                    if name == self.section and not query:
                        button.set_state_flags(Gtk.StateFlags.CHECKED, False)
                    else:
                        button.unset_state_flags(Gtk.StateFlags.CHECKED)
            for child in self.sections.get_children():
                child.destroy()
            self.buttons = []
            self.button_items = {}
            self.rows = []
            self.selected_button = None
            for title, items in sections:
                section = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
                heading = Gtk.Label(label=title, xalign=0, margin_start=4)
                heading.get_style_context().add_class('heading')
                section.pack_start(heading, False, False, 0)
                flow = Gtk.FlowBox(homogeneous=True, selection_mode=Gtk.SelectionMode.NONE,
                                   min_children_per_line=8, max_children_per_line=8,
                                   row_spacing=2, column_spacing=2, valign=Gtk.Align.START)
                group_buttons = []
                for item in items:
                    chosen = lookup.get(state.get('tones', {}).get(base_key(item['text'])), item) if title not in ('Recently Used', 'Favourites') else item
                    button = Gtk.Button(label=chosen['text'])
                    button.get_child().set_max_width_chars(1)
                    button.get_child().set_ellipsize(Pango.EllipsizeMode.END)
                    button.get_style_context().add_class('character' if chosen.get('symbol') else 'emoji')
                    button.set_tooltip_text(chosen['name'])
                    button.get_accessible().set_name(chosen['name'])
                    button.connect('clicked', self.activate_item, chosen)
                    button.connect('button-press-event', self.context, chosen)
                    button.connect('focus-in-event', lambda b, e, i=chosen: self.inspect_item(b, i))
                    gesture = Gtk.GestureLongPress.new(button)
                    gesture.connect('pressed', lambda g, x, y, b=button, i=chosen: self.options(b, i))
                    button._long_press = gesture
                    flow.add(button)
                    group_buttons.append(button)
                    self.buttons.append(button)
                    self.button_items[button] = chosen
                self.rows.extend(group_buttons[n:n+8] for n in range(0, len(group_buttons), 8))
                section.pack_start(flow, False, False, 0)
                self.sections.pack_start(section, False, False, 0)
            self.sections.show_all()
            self.empty.set_text('No characters found' if query else 'Add favourites from a character’s options')
            self.empty.set_visible(not self.buttons)
            self.scroll.set_visible(bool(self.buttons))
            self.scroll.get_vadjustment().set_value(0)
            selected = next((b for b in self.buttons if self.selected and self.button_items[b]['text'] == self.selected['text']), None)
            self.inspect_item(selected, self.button_items[selected] if selected else None)
            return False

        def inspect_item(self, button, item):
            if self.selected_button:
                self.selected_button.get_style_context().remove_class('selected')
            self.selected, self.selected_button = item, button
            if button and self.expanded:
                button.get_style_context().add_class('selected')
            style = self.preview.get_style_context()
            if item and item.get('symbol'):
                style.add_class('text-preview')
            else:
                style.remove_class('text-preview')
            self.preview.set_text(item['text'] if item else '')
            self.detail_name.set_text(item['name'] if item else 'Select a character')
            self.detail_code.set_text(' '.join(f'U+{ord(c):04X}' for c in item['text']) if item else '')
            for action in self.detail_actions:
                action.set_sensitive(bool(item))
            self.detail_favorite.set_label('Remove from Favourites' if item and item['text'] in state.get('favorites', []) else 'Add to Favourites')
            return False

        def activate_item(self, button, item):
            if self.popover and self.popover.get_visible():
                return
            if self.expanded:
                self.inspect_item(button, item)
            else:
                self.choose(item)

        def context(self, button, event, item):
            if self.expanded and event.button == 1 and event.type == Gdk.EventType._2BUTTON_PRESS:
                self.choose(item)
                return True
            if event.button == 3:
                self.options(button, item)
                return True
            return False

        def options(self, button, item):
            if self.popover:
                self.popover.destroy()
            self.popover = Gtk.Popover.new(button)
            self.popover.set_name('character-options')
            self.popover.set_position(Gtk.PositionType.TOP)
            box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, margin=10)
            title = Gtk.Label(label=item['name'], wrap=True, max_width_chars=28)
            box.pack_start(title, False, False, 0)
            choices = variants.get(base_key(item['text']), [])
            if len(choices) > 1:
                # Include all valid mixed-tone sequences in a wrapping grid.
                tones = Gtk.FlowBox(min_children_per_line=6, max_children_per_line=6, selection_mode=Gtk.SelectionMode.NONE)
                for variant in choices:
                    btn = Gtk.Button(label=variant['text'])
                    btn.get_style_context().add_class('emoji')
                    btn.set_tooltip_text(variant['name'])
                    btn.get_accessible().set_name(variant['name'])
                    btn.connect('clicked', lambda b, i=variant: self.choose_tone(i))
                    tones.add(btn)
                tone_scroll = Gtk.ScrolledWindow()
                tone_scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
                tone_scroll.set_min_content_width(228)
                tone_scroll.set_min_content_height(min(190, ((len(choices)+5)//6)*40))
                tone_scroll.set_max_content_height(190)
                tone_scroll.add(tones)
                box.pack_start(tone_scroll, False, False, 0)
            favorites = state.setdefault('favorites', [])
            favorite = Gtk.Button(label='Remove from Favourites' if item['text'] in favorites else 'Add to Favourites')
            favorite.connect('clicked', self.favorite, item)
            box.pack_start(favorite, False, False, 0)
            copy = Gtk.Button(label='Copy')
            copy.connect('clicked', lambda b: self.choose(item, copy_only=True))
            box.pack_start(copy, False, False, 0)
            self.popover.add(box)
            self.popover.show_all()
            self.popover.popup()

        def favorite(self, button, item):
            favorites = state.setdefault('favorites', [])
            if item['text'] in favorites:
                favorites.remove(item['text'])
            else:
                favorites.append(item['text'])
            save_state(state)
            if self.popover:
                self.popover.popdown()
            self.render()

        def choose_tone(self, item):
            state.setdefault('tones', {})[base_key(item['text'])] = item['text']
            self.choose(item)

        def key(self, widget, event):
            focus = self.get_focus()
            if event.keyval in (Gdk.KEY_f, Gdk.KEY_F) and event.state & Gdk.ModifierType.CONTROL_MASK:
                self.search.grab_focus()
                self.search.select_region(0, -1)
                return True
            if (event.keyval == Gdk.KEY_Menu or (event.keyval == Gdk.KEY_F10 and event.state & Gdk.ModifierType.SHIFT_MASK)) and focus in self.button_items:
                self.options(focus, self.button_items[focus])
                return True
            if event.keyval == Gdk.KEY_Escape:
                if self.popover and self.popover.get_visible():
                    self.popover.popdown()
                else:
                    self.destroy()
                return True
            if event.keyval == Gdk.KEY_Down and self.get_focus() is self.search and self.buttons:
                self.buttons[0].grab_focus()
                return True
            if not (self.popover and self.popover.get_visible()) and focus in self.button_items:
                if event.keyval in (Gdk.KEY_Return, Gdk.KEY_KP_Enter):
                    self.choose(self.button_items[focus])
                    return True
                if event.keyval in (Gdk.KEY_Left, Gdk.KEY_Right, Gdk.KEY_Up, Gdk.KEY_Down):
                    row = next(n for n, buttons in enumerate(self.rows) if focus in buttons)
                    col = self.rows[row].index(focus)
                    if event.keyval in (Gdk.KEY_Left, Gdk.KEY_Right):
                        index = self.buttons.index(focus) + (-1 if event.keyval == Gdk.KEY_Left else 1)
                        self.buttons[max(0, min(len(self.buttons)-1, index))].grab_focus()
                    elif event.keyval == Gdk.KEY_Up and row == 0:
                        self.search.grab_focus()
                    else:
                        row = max(0, min(len(self.rows)-1, row+(-1 if event.keyval == Gdk.KEY_Up else 1)))
                        self.rows[row][min(col, len(self.rows[row])-1)].grab_focus()
                    return True
                if event.string and event.string.isprintable() and not event.state & (Gdk.ModifierType.CONTROL_MASK | Gdk.ModifierType.MOD1_MASK | Gdk.ModifierType.SUPER_MASK):
                    self.search.grab_focus()
                    self.search.set_text(self.search.get_text()+event.string)
                    self.search.set_position(-1)
                    return True
            if event.keyval in (Gdk.KEY_Return, Gdk.KEY_KP_Enter) and isinstance(self.get_focus(), Gtk.Button):
                self.get_focus().clicked()
                return True
            return False

        def enter(self, *_):
            if self.buttons:
                self.choose(self.button_items[self.buttons[0]])

        def choose(self, item, copy_only=False):
            target = self.target
            if self.choosing:
                return
            self.choosing = True
            state['recent'] = ([item['text']] + [t for t in state.get('recent', []) if t != item['text']])[:40]
            save_state(state)
            if self.native:
                if copy_only:
                    sp.run(['wl-copy', '--', item['text']], check=True)
                elif not bridge.cv_commit(item['text'].encode(), self.generation):
                    sp.run(['wl-copy', '--', item['text']], check=True)
                    sp.run(['notify-send', 'Emoji copied', 'The original text field is no longer active. Paste to insert.'], check=False)
                self.destroy()
                return
            self.hide()
            if self.popover:
                self.popover.hide()
            def insert():
                try:
                    if copy_only or not target:
                        sp.run(['wl-copy', '--', item['text']], check=True)
                    else:
                        sway(f'[con_id={target["id"]}] focus')
                        current = focused(sway('-t', 'get_tree'))
                        if not current or current['id'] != target['id']:
                            raise RuntimeError('The original window has closed.')
                        app = (target.get('app_id') or target.get('window_properties', {}).get('class', '')).lower()
                        if any(name in app for name in ('brave', 'chromium', 'google-chrome', 'codex-desktop', 'chatgpt')):
                            # Chromium may silently ignore non-BMP virtual keysyms.
                            # Paste Unicode through its normal text input path instead.
                            sp.run(['wl-copy', '--', item['text']], check=True)
                            sp.run(['wtype', '-s', '100', '-M', 'ctrl', '-k', 'v', '-m', 'ctrl'], check=True, timeout=3)
                        else:
                            # Virtual keyboard text leaves the existing clipboard intact.
                            sp.run(['wtype', '-s', '100', '-d', '10', '--', item['text']], check=True, timeout=3)
                except (sp.SubprocessError, OSError, RuntimeError) as error:
                    sp.run(['wl-copy', '--', item['text']], check=False)
                    sp.run(['notify-send', 'Emoji copied', 'Could not insert into the original app. Paste to insert.'], check=False)
                    print(error)
                self.destroy()
                return False
            GLib.timeout_add(180, insert)

    def changed():
        # Reset when the application changes fields; never commit into a new field.
        if current_window and current_window.native and (
                not bridge.cv_active() or bridge.cv_epoch() != current_window.generation):
            GLib.idle_add(current_window.destroy)

    if daemon:
        bridge = ctypes.CDLL(str(HERE / 'libcharacter-input.so'))
        callback_type = ctypes.CFUNCTYPE(None)
        callback = callback_type(changed)
        bridge.cv_init.argtypes = [callback_type]
        bridge.cv_attach.argtypes = [ctypes.c_void_p]
        bridge.cv_commit.argtypes = [ctypes.c_char_p, ctypes.c_uint]
        bridge.cv_epoch.restype = ctypes.c_uint
        replacement_type=ctypes.CFUNCTYPE(None,ctypes.c_char_p,ctypes.c_uint,ctypes.c_uint,ctypes.c_uint)
        bridge.cv_replace.argtypes=[ctypes.c_uint,ctypes.c_char_p,ctypes.c_uint,ctypes.c_uint]
        bridge.cv_replacement_callback.argtypes=[replacement_type]
        def expand(data,cursor,epoch,serial):
            try:
                from replacements import replacement_for
                value=replacement_for(data[:cursor].decode('utf-8'))
                if value:bridge.cv_replace(value[0],value[1].encode('utf-8'),epoch,serial)
            except (OSError,ValueError,UnicodeError):pass
        expansion_callback=replacement_type(expand)
        bridge.cv_replacement_callback(expansion_callback)
        bridge.cv_init(callback)
        server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        ipc_path().unlink(missing_ok=True)
        server.bind(str(ipc_path()))
        ipc_path().chmod(0o600)
        server.listen(4)
        server.setblocking(False)

        def receive(source, condition):
            nonlocal target, current_window, native, generation, state
            client, _ = server.accept()
            try:
                client.settimeout(0.2)
                command = client.recv(32).decode()
                if command == 'status':
                    result = {'running': True, 'available': bool(bridge.cv_available()),
                              'active': bool(bridge.cv_active()),
                              'visible': bool(current_window),
                              'mode': 'caret' if current_window and current_window.native else 'window'}
                    client.sendall(json.dumps(result).encode())
                elif command == 'show':
                    if current_window:
                        current_window.destroy()
                    else:
                        target = focused(sway('-t', 'get_tree'))
                        state = load_state()
                        native = bool(bridge.cv_active())
                        generation = bridge.cv_epoch()
                        current_window = Viewer()
                    client.sendall(b'ok')
            except (OSError, ValueError) as error:
                print(error, file=sys.stderr)
            finally:
                client.close()
            return True
        GLib.io_add_watch(server.fileno(), GLib.PRIORITY_DEFAULT, GLib.IO_IN, receive)
        try:
            Gtk.main()
        finally:
            ipc_path().unlink(missing_ok=True)
    else:
        current_window = Viewer()
        Gtk.main()


if __name__ == '__main__':
    main()
