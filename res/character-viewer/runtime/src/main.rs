//! GTK window for the standalone character viewer. The input bridge and daemon follow.
mod target;
mod daemon;
mod bridge;
mod appearance;
use bridge::Bridge;
use character_viewer_catalogue::{base_key, catalogue, load_state, render_sections, save_state};
use gtk::prelude::*;
use gtk::glib::translate::*;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use std::io::ErrorKind;

struct Viewer {
    window: gtk::Window,
    data: Value,
    state: RefCell<Value>,
    state_path: PathBuf,
    section: RefCell<String>,
    expanded: Cell<bool>,
    search: gtk::SearchEntry,
    scroll: gtk::ScrolledWindow,
    empty: gtk::Label,
    sections: gtk::Box,
    categories: RefCell<Vec<(String,gtk::ToggleButton)>>,
    sidebar_buttons: RefCell<Vec<(String,gtk::Button)>>,
    sidebar: gtk::ScrolledWindow,
    details: gtk::Box,
    preview: gtk::Label,
    name: gtk::Label,
    code: gtk::Label,
    insert_button: gtk::Button,
    copy_button: gtk::Button,
    favorite_button: gtk::Button,
    tones: gtk::FlowBox,
    selected: RefCell<Option<Value>>,
    first_button: RefCell<Option<gtk::Button>>,
    buttons: RefCell<Vec<(gtk::Button,Value)>>,
    popover: RefCell<Option<gtk::Popover>>,
    target: Option<Value>,
    bridge: Option<Rc<Bridge>>,
    native: bool,
    generation: u32,
    choosing: Cell<bool>,
}

impl Viewer {
    fn new(data: Value, state_path: PathBuf, target: Option<Value>,
        daemon_slot:Option<Rc<RefCell<Option<Rc<Self>>>>>,bridge:Option<Rc<Bridge>>,
        force_window:bool) -> Rc<Self> {
        let native=!force_window && bridge.as_ref().is_some_and(|bridge|bridge.active());
        let generation=bridge.as_ref().map_or(0,|bridge|bridge.epoch());
        let state=load_state(&state_path);
        let expanded=state["expanded"].as_bool().unwrap_or(false);
        let dimensions=target::viewer_dimensions(target.as_ref(),expanded,native);
        let window=gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("Emoji & Symbols");window.set_decorated(false);window.set_resizable(false);
        window.set_type_hint(gtk::gdk::WindowTypeHint::Dialog);
        if let Some(visual)=gtk::prelude::WidgetExt::screen(&window).and_then(|screen|screen.rgba_visual()) {
            window.set_visual(Some(&visual));
        }
        window.set_default_size(dimensions.width,dimensions.height);
        let outer=gtk::Box::new(gtk::Orientation::Vertical,6);
        outer.set_widget_name("panel");window.add(&outer);
        let header=gtk::Box::new(gtk::Orientation::Horizontal,8);
        let search=gtk::SearchEntry::new();search.set_placeholder_text(Some("Search"));
        header.pack_start(&search,true,true,0);
        let expand=gtk::Button::new();
        expand.add(&gtk::Image::from_icon_name(Some("character-viewer-symbolic"),gtk::IconSize::Menu));
        expand.set_tooltip_text(Some("Expand / collapse Character Viewer"));
        if let Some(accessible)=expand.accessible() {accessible.set_name("Expand / collapse Character Viewer");}
        header.pack_start(&expand,false,false,0);outer.pack_start(&header,false,false,8);
        let content=gtk::Box::new(gtk::Orientation::Horizontal,0);
        let sidebar=gtk::ScrolledWindow::new(None::<&gtk::Adjustment>,None::<&gtk::Adjustment>);
        sidebar.style_context().add_class("sidebar");
        sidebar.set_size_request(166,-1);
        let side_items=gtk::Box::new(gtk::Orientation::Vertical,0);sidebar.add(&side_items);
        content.pack_start(&sidebar,false,false,0);
        let grid_box=gtk::Box::new(gtk::Orientation::Vertical,6);
        grid_box.set_margin_start(10);grid_box.set_margin_end(10);
        let scroll=gtk::ScrolledWindow::new(None::<&gtk::Adjustment>,None::<&gtk::Adjustment>);
        scroll.set_size_request(330,272);
        scroll.set_min_content_width(330);scroll.set_max_content_width(330);
        scroll.set_propagate_natural_width(false);
        scroll.set_policy(gtk::PolicyType::Never,gtk::PolicyType::Automatic);
        let sections=gtk::Box::new(gtk::Orientation::Vertical,10);scroll.add(&sections);
        grid_box.pack_start(&scroll,true,true,0);
        let empty=gtk::Label::new(Some("No characters found"));empty.set_margin_top(24);empty.set_margin_bottom(24);
        empty.set_no_show_all(true);scroll.set_no_show_all(true);
        grid_box.pack_start(&empty,false,false,0);
        content.pack_start(&grid_box,true,true,0);
        let details=gtk::Box::new(gtk::Orientation::Vertical,8);details.set_size_request(170,-1);
        details.style_context().add_class("details");
        let preview=gtk::Label::new(None);let name=gtk::Label::new(Some("Select a character"));
        preview.style_context().add_class("preview");
        name.style_context().add_class("detail-name");
        details.pack_start(&preview,false,false,8);details.pack_start(&name,false,false,0);
        let code=gtk::Label::new(None);code.set_selectable(true);
        code.style_context().add_class("hint");details.pack_start(&code,false,false,0);
        let tones=gtk::FlowBox::new();tones.set_max_children_per_line(6);
        tones.set_selection_mode(gtk::SelectionMode::None);
        details.pack_start(&tones,false,false,0);
        let insert_button=gtk::Button::with_label("Insert");
        let copy_button=gtk::Button::with_label("Copy");
        let favorite_button=gtk::Button::with_label("Add to Favourites");
        for button in [&insert_button,&copy_button,&favorite_button] {
            button.style_context().add_class("action");
            button.set_sensitive(false);details.pack_start(button,false,false,0);
        }
        content.pack_start(&details,false,false,0);outer.pack_start(&content,true,true,0);
        let toolbar=gtk::Box::new(gtk::Orientation::Horizontal,0);toolbar.set_homogeneous(true);
        toolbar.style_context().add_class("toolbar");
        outer.pack_start(&toolbar,false,false,0);
        let viewer=Rc::new(Self{window,data,state:RefCell::new(state),state_path,
            section:RefCell::new("Recent".into()),expanded:Cell::new(expanded),search,scroll,empty,
            sections,categories:RefCell::new(Vec::new()),sidebar_buttons:RefCell::new(Vec::new()),sidebar,details,preview,name,code,insert_button,copy_button,favorite_button,tones,
            selected:RefCell::new(None),first_button:RefCell::new(None),buttons:RefCell::new(Vec::new()),popover:RefCell::new(None),target,bridge:bridge.clone(),native,generation,choosing:Cell::new(false)});
        for section in ["Recent","Favorites","Smileys & People","Animals & Nature","Food & Drink",
            "Activities","Travel & Places","Objects","Symbols","Flags","Arrows",
            "Currency Symbols","Latin","Letterlike Symbols","Maths Symbols","Punctuation"] {
            let button=gtk::Button::with_label(section);
            let weak=Rc::downgrade(&viewer);
            button.connect_clicked(move |_| { if let Some(viewer)=weak.upgrade() {
                *viewer.section.borrow_mut()=section.into();viewer.search.set_text("");viewer.render();
            }});
            viewer.sidebar_buttons.borrow_mut().push((section.into(),button.clone()));
            side_items.pack_start(&button,false,false,0);
        }
        for (section,icon) in [("Recent","emoji-recent"),("Favorites","starred"),
            ("Smileys & People","emoji-people"),("Animals & Nature","emoji-nature"),
            ("Food & Drink","emoji-food"),("Activities","emoji-activities"),
            ("Travel & Places","emoji-travel"),("Objects","emoji-objects"),
            ("Symbols","emoji-symbols"),("Flags","emoji-flags")] {
            let button=gtk::ToggleButton::new();
            button.add(&gtk::Image::from_icon_name(Some(&format!("{icon}-symbolic")),gtk::IconSize::Menu));
            button.style_context().add_class("category");
            button.set_tooltip_text(Some(section));
            if let Some(accessible)=button.accessible() {accessible.set_name(section);}
            let weak=Rc::downgrade(&viewer);
            button.connect_clicked(move |_| {if let Some(viewer)=weak.upgrade() {
                *viewer.section.borrow_mut()=section.into();viewer.search.set_text("");viewer.render();
            }});
            viewer.categories.borrow_mut().push((section.into(),button.clone()));
            toolbar.pack_start(&button,true,true,0);
        }
        let weak=Rc::downgrade(&viewer);
        viewer.search.connect_search_changed(move |_| {if let Some(viewer)=weak.upgrade(){viewer.render();}});
        let weak=Rc::downgrade(&viewer);
        expand.connect_clicked(move |_| {if let Some(viewer)=weak.upgrade(){viewer.toggle_expanded();}});
        let weak=Rc::downgrade(&viewer);
        viewer.insert_button.connect_clicked(move |_| {if let Some(viewer)=weak.upgrade() {
            if let Some(item)=viewer.selected.borrow().clone(){viewer.choose(&item,false);}
        }});
        let weak=Rc::downgrade(&viewer);
        viewer.copy_button.connect_clicked(move |_| {if let Some(viewer)=weak.upgrade() {
            if let Some(item)=viewer.selected.borrow().clone(){viewer.choose(&item,true);}
        }});
        let weak=Rc::downgrade(&viewer);
        viewer.favorite_button.connect_clicked(move |_| {if let Some(viewer)=weak.upgrade() {
            if let Some(item)=viewer.selected.borrow().clone(){viewer.toggle_favorite(&item);}
        }});
        let weak=Rc::downgrade(&viewer);
        viewer.search.connect_activate(move |_| {if let Some(viewer)=weak.upgrade(){viewer.choose_first();}});
        let weak=Rc::downgrade(&viewer);
        viewer.window.connect_key_press_event(move |_,event| {
            let Some(viewer)=weak.upgrade() else {return gtk::glib::Propagation::Proceed};
            match event.keyval() {
                gtk::gdk::keys::constants::Escape => {
                    if let Some(popover)=viewer.popover.borrow().as_ref().filter(|popover|popover.is_visible()) {
                        popover.popdown();
                    } else {viewer.close();}
                    gtk::glib::Propagation::Stop
                }
                gtk::gdk::keys::constants::Return | gtk::gdk::keys::constants::KP_Enter
                    if viewer.native => {
                        if let Some((_,item))=viewer.focused_choice() {viewer.choose(&item,false);}
                        else {viewer.choose_first();}
                        gtk::glib::Propagation::Stop
                    }
                gtk::gdk::keys::constants::BackSpace if viewer.native => {
                    let mut value=viewer.search.text().to_string();value.pop();viewer.search.set_text(&value);
                    gtk::glib::Propagation::Stop
                }
                gtk::gdk::keys::constants::Down if viewer.search.has_focus() => {
                    if let Some(button)=viewer.first_button.borrow().as_ref() {button.grab_focus();}
                    gtk::glib::Propagation::Stop
                }
                gtk::gdk::keys::constants::Menu => {
                    if let Some((button,item))=viewer.focused_choice() {
                        viewer.options(&button,&item);gtk::glib::Propagation::Stop
                    } else {gtk::glib::Propagation::Proceed}
                }
                gtk::gdk::keys::constants::F10
                    if event.state().contains(gtk::gdk::ModifierType::SHIFT_MASK) => {
                    if let Some((button,item))=viewer.focused_choice() {
                        viewer.options(&button,&item);gtk::glib::Propagation::Stop
                    } else {gtk::glib::Propagation::Proceed}
                }
                gtk::gdk::keys::constants::Left | gtk::gdk::keys::constants::Right |
                gtk::gdk::keys::constants::Up | gtk::gdk::keys::constants::Down
                    if viewer.focused_choice().is_some() => {
                    viewer.move_choice(event.keyval());gtk::glib::Propagation::Stop
                }
                gtk::gdk::keys::constants::f | gtk::gdk::keys::constants::F
                    if event.state().contains(gtk::gdk::ModifierType::CONTROL_MASK) => {
                    viewer.search.grab_focus();viewer.search.select_region(0,-1);
                    gtk::glib::Propagation::Stop
                }
                _=>{
                    let blocked=gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::MOD1_MASK |
                        gtk::gdk::ModifierType::SUPER_MASK;
                    if viewer.native && !event.state().intersects(blocked) {
                        if let Some(ch)=event.keyval().to_unicode().filter(|ch|!ch.is_control()) {
                            let value=format!("{}{}",viewer.search.text(),ch);
                            viewer.search.set_text(&value);viewer.search.set_position(-1);
                            return gtk::glib::Propagation::Stop;
                        }
                    }
                    gtk::glib::Propagation::Proceed
                }
            }
        });
        let weak=Rc::downgrade(&viewer);
        viewer.window.connect_focus_out_event(move |_,_| {
            let weak=weak.clone();
            gtk::glib::timeout_add_local_once(Duration::from_millis(150),move || {
                let Some(viewer)=weak.upgrade() else {return};
                if viewer.native || viewer.choosing.get() || viewer.window.is_active() {return;}
                if viewer.popover.borrow().as_ref().is_some_and(|popover|popover.is_visible()) {return;}
                viewer.close();
            });
            gtk::glib::Propagation::Proceed
        });
        if native {
            let input=bridge.as_ref().unwrap().clone();
            let weak=Rc::downgrade(&viewer);
            viewer.window.connect_realize(move |window| {
                if !input.attach(window.as_ptr() as *mut _) {
                    let weak=weak.clone();
                    gtk::glib::idle_add_local_once(move || {if let Some(viewer)=weak.upgrade(){viewer.close();}});
                }
            });
            let input=bridge.as_ref().unwrap().clone();
            viewer.window.connect_unrealize(move |_|input.hide());
            let input=bridge.as_ref().unwrap().clone();
            viewer.window.connect_destroy(move |_|input.hide());
        } else {
            let weak=Rc::downgrade(&viewer);
            viewer.window.connect_map_event(move |_,_| {
                let weak=weak.clone();
                gtk::glib::timeout_add_local_once(Duration::from_millis(50),move || {
                    if let Some(viewer)=weak.upgrade() {viewer.reposition();}
                });
                gtk::glib::Propagation::Proceed
            });
        }
        if let Some(slot)=daemon_slot.clone() {
            viewer.window.connect_destroy(move |_| {*slot.borrow_mut()=None;});
        } else {
            viewer.window.connect_destroy(|_|gtk::main_quit());
        }
        viewer.sidebar.set_no_show_all(!dimensions.sidebar);viewer.details.set_no_show_all(!dimensions.details);
        viewer.sidebar.set_visible(dimensions.sidebar);viewer.details.set_visible(dimensions.details);
        viewer.render();
        if native {viewer.window.set_opacity(0.0);}
        viewer.window.show_all();
        viewer.search.grab_focus();
        if native {
            viewer.synthesize_focus();
            let weak=Rc::downgrade(&viewer);
            gtk::glib::timeout_add_local_once(Duration::from_millis(120),move || {
                let Some(viewer)=weak.upgrade() else {return};
                let Some(bridge)=viewer.bridge.as_ref() else {return};
                if bridge.positioned() {viewer.window.set_opacity(1.0);return;}
                let Some(slot)=daemon_slot else {viewer.close();return};
                let data=viewer.data.clone();let state_path=viewer.state_path.clone();
                let target=viewer.target.clone();let bridge=viewer.bridge.clone();
                viewer.close();
                let fallback=Viewer::new(data,state_path,target,Some(slot.clone()),bridge,true);
                *slot.borrow_mut()=Some(fallback);
            });
        }
        viewer
    }

    fn synthesize_focus(&self) {
        let Some(gdk_window)=self.window.window() else {return};
        let event=gtk::gdk::Event::new(gtk::gdk::EventType::FocusChange);
        unsafe {
            let raw:*mut gtk::gdk::ffi::GdkEvent=event.to_glib_none().0;
            (*raw).focus_change.window=gdk_window.to_glib_full();
            (*raw).focus_change.send_event=1;
            (*raw).focus_change.in_=1;
        }
        self.window.event(&event);
    }

    fn close(&self) {
        if self.native {if let Some(bridge)=self.bridge.as_ref(){bridge.hide();}}
        self.window.close();
    }

    fn toggle_expanded(self:&Rc<Self>) {
        let expanded=!self.expanded.get();self.expanded.set(expanded);
        self.state.borrow_mut()["expanded"]=json!(expanded);
        let _=save_state(&self.state_path,&self.state.borrow());
        let dimensions=target::viewer_dimensions(self.target.as_ref(),expanded,self.native);
        self.sidebar.set_no_show_all(!dimensions.sidebar);self.details.set_no_show_all(!dimensions.details);
        self.sidebar.set_visible(dimensions.sidebar);self.details.set_visible(dimensions.details);
        if expanded {self.show_expanded_panes();}
        self.window.resize(dimensions.width,dimensions.height);
        self.render();
        if !self.native {self.reposition();}
    }

    fn show_expanded_panes(&self) {
        if self.sidebar.is_visible() {self.sidebar.show_all();}
        if self.details.is_visible() {self.details.show_all();}
    }

    fn reposition(self:&Rc<Self>) {
        let dimensions=target::viewer_dimensions(self.target.as_ref(),self.expanded.get(),self.native);
        let (width,height)=(dimensions.width,dimensions.height);
        if let Err(error)=target::position_window(width,height) {eprintln!("Viewer placement: {error}");}
        let weak=Rc::downgrade(self);let attempts=Rc::new(Cell::new(0));
        gtk::glib::timeout_add_local(Duration::from_millis(10),move || {
            let Some(viewer)=weak.upgrade() else {return gtk::glib::ControlFlow::Break};
            if !viewer.window.is_visible() {return gtk::glib::ControlFlow::Break};
            match target::reveal_window(width,height) {
                Ok(true)=>gtk::glib::ControlFlow::Break,
                Ok(false)=>{
                    attempts.set(attempts.get()+1);
                    if attempts.get()<200 {gtk::glib::ControlFlow::Continue}
                    else {gtk::glib::ControlFlow::Break}
                }
                Err(error)=>{eprintln!("Viewer placement: {error}");gtk::glib::ControlFlow::Break}
            }
        });
    }

    fn render(self:&Rc<Self>) {
        let selected_section=self.section.borrow().clone();
        let no_query=self.search.text().trim().is_empty();
        for (name,button) in self.categories.borrow().iter() {
            button.set_active(no_query && *name==selected_section);
        }
        for (name,button) in self.sidebar_buttons.borrow().iter() {
            if no_query && *name==selected_section {
                button.set_state_flags(gtk::StateFlags::CHECKED,false);
            } else {button.unset_state_flags(gtk::StateFlags::CHECKED);}
        }
        for child in self.sections.children() {self.sections.remove(&child);}
        *self.first_button.borrow_mut()=None;
        self.buttons.borrow_mut().clear();
        let data=render_sections(&self.data,&self.state.borrow(),&self.section.borrow(),
            &self.search.text(),self.expanded.get());
        if let Some(groups)=data.as_array() {
            for group in groups {
                let heading=gtk::Label::new(group["title"].as_str());heading.set_xalign(0.0);
                heading.style_context().add_class("heading");
                self.sections.pack_start(&heading,false,false,0);
                let flow=gtk::FlowBox::new();flow.set_max_children_per_line(8);flow.set_min_children_per_line(1);
                if let Some(items)=group["items"].as_array() {
                    for item in items {
                        let button=gtk::Button::with_label(item["text"].as_str().unwrap_or(""));
                        button.style_context().add_class(if item["symbol"]==true {"character"} else {"emoji"});
                        button.set_tooltip_text(item["name"].as_str());
                        if let (Some(accessible),Some(name))=(button.accessible(),item["name"].as_str()) {
                            accessible.set_name(name);
                        }
                        let first_missing=self.first_button.borrow().is_none();
                        if first_missing {*self.first_button.borrow_mut()=Some(button.clone());}
                        self.buttons.borrow_mut().push((button.clone(),item.clone()));
                        let weak=Rc::downgrade(self);let choice=item.clone();
                        button.connect_clicked(move |_| {if let Some(viewer)=weak.upgrade(){viewer.activate(&choice);}});
                        let weak=Rc::downgrade(self);let choice=item.clone();
                        button.connect_button_press_event(move |button,event| {
                            let Some(viewer)=weak.upgrade() else {return gtk::glib::Propagation::Proceed};
                            if event.button()==3 {
                                viewer.options(button,&choice);
                                gtk::glib::Propagation::Stop
                            } else if viewer.expanded.get() && event.button()==1 &&
                                event.event_type()==gtk::gdk::EventType::DoubleButtonPress {
                                viewer.choose(&choice,false);
                                gtk::glib::Propagation::Stop
                            } else {gtk::glib::Propagation::Proceed}
                        });
                        flow.add(&button);
                    }
                }
                self.sections.pack_start(&flow,false,false,0);
            }
        }
        self.sections.show_all();
        if self.expanded.get() {
            if let Some(selected)=self.selected.borrow().as_ref() {
                if let Some((button,_))=self.buttons.borrow().iter().find(|(_,item)|item["text"]==selected["text"]) {
                    button.style_context().add_class("selected");
                }
            }
        }
        let has_results=!self.buttons.borrow().is_empty();
        self.empty.set_text(if self.search.text().trim().is_empty() {
            "Add favourites from a character’s options"
        } else {"No characters found"});
        self.empty.set_visible(!has_results);
        self.scroll.set_visible(has_results);
        self.scroll.vadjustment().set_value(0.0);
    }

    fn focused_choice(&self)->Option<(gtk::Button,Value)> {
        let focus=self.window.focused_widget()?;
        self.buttons.borrow().iter().find(|(button,_)|focus==button.clone().upcast::<gtk::Widget>()).cloned()
    }

    fn move_choice(&self,key:gtk::gdk::keys::Key) {
        let Some(focus)=self.window.focused_widget() else {return};
        let buttons=self.buttons.borrow();
        let Some(index)=buttons.iter().position(|(button,_)|focus==button.clone().upcast::<gtk::Widget>()) else {return};
        if key==gtk::gdk::keys::constants::Up && index<8 {self.search.grab_focus();return;}
        let next=match key {
            gtk::gdk::keys::constants::Left=>index.saturating_sub(1),
            gtk::gdk::keys::constants::Right=>(index+1).min(buttons.len()-1),
            gtk::gdk::keys::constants::Up=>index.saturating_sub(8),
            _=>(index+8).min(buttons.len()-1),
        };
        buttons[next].0.grab_focus();
    }

    fn activate(self:&Rc<Self>,item:&Value) {
        if self.popover.borrow().as_ref().is_some_and(|popover|popover.is_visible()) {return;}
        for (button,choice) in self.buttons.borrow().iter() {
            if self.expanded.get() && choice["text"]==item["text"] {
                button.style_context().add_class("selected");
            } else {button.style_context().remove_class("selected");}
        }
        *self.selected.borrow_mut()=Some(item.clone());
        if item["symbol"]==true {self.preview.style_context().add_class("text-preview");}
        else {self.preview.style_context().remove_class("text-preview");}
        self.preview.set_text(item["text"].as_str().unwrap_or(""));
        self.name.set_text(item["name"].as_str().unwrap_or(""));
        self.code.set_text(&item["text"].as_str().unwrap_or("").chars()
            .map(|ch|format!("U+{:04X}",ch as u32)).collect::<Vec<_>>().join(" "));
        for button in [&self.insert_button,&self.copy_button,&self.favorite_button] {button.set_sensitive(true);}
        let saved=self.state.borrow()["favorites"].as_array().is_some_and(|items|items.contains(&item["text"]));
        self.favorite_button.set_label(if saved {"Remove from Favourites"} else {"Add to Favourites"});
        for child in self.tones.children() {self.tones.remove(&child);}
        if let Some(variants)=self.data["variants"][base_key(item["text"].as_str().unwrap_or(""))].as_array() {
            if variants.len()>1 {
                for variant in variants {
                    let button=gtk::Button::with_label(variant["text"].as_str().unwrap_or(""));
                    button.set_tooltip_text(variant["name"].as_str());
                    if let (Some(accessible),Some(name))=(button.accessible(),variant["name"].as_str()) {
                        accessible.set_name(name);
                    }
                    let weak=Rc::downgrade(self);let variant=variant.clone();
                    button.connect_clicked(move |_| {if let Some(viewer)=weak.upgrade(){viewer.choose_tone(&variant);}});
                    self.tones.add(&button);
                }
            }
        }
        self.tones.show_all();
        if !self.expanded.get() { self.choose(item,false); }
    }

    fn options(self:&Rc<Self>,button:&gtk::Button,item:&Value) {
        if let Some(previous)=self.popover.borrow_mut().take(){previous.popdown();}
        let popover=gtk::Popover::new(Some(button));
        popover.set_widget_name("character-options");
        popover.set_position(gtk::PositionType::Top);
        let box_=gtk::Box::new(gtk::Orientation::Vertical,8);
        box_.set_margin_top(10);box_.set_margin_bottom(10);
        box_.set_margin_start(10);box_.set_margin_end(10);
        let title=gtk::Label::new(item["name"].as_str());title.set_line_wrap(true);
        title.set_max_width_chars(28);box_.pack_start(&title,false,false,0);
        if let Some(variants)=self.data["variants"][base_key(item["text"].as_str().unwrap_or(""))].as_array() {
            if variants.len()>1 {
                let tones=gtk::FlowBox::new();tones.set_min_children_per_line(6);
                tones.set_max_children_per_line(6);tones.set_selection_mode(gtk::SelectionMode::None);
                for variant in variants {
                    let choice=gtk::Button::with_label(variant["text"].as_str().unwrap_or(""));
                    choice.style_context().add_class("emoji");
                    choice.set_tooltip_text(variant["name"].as_str());
                    if let (Some(accessible),Some(name))=(choice.accessible(),variant["name"].as_str()) {
                        accessible.set_name(name);
                    }
                    let weak=Rc::downgrade(self);let variant=variant.clone();
                    choice.connect_clicked(move |_| {if let Some(viewer)=weak.upgrade(){viewer.choose_tone(&variant);}});
                    tones.add(&choice);
                }
                let scroll=gtk::ScrolledWindow::new(None::<&gtk::Adjustment>,None::<&gtk::Adjustment>);
                scroll.set_policy(gtk::PolicyType::Never,gtk::PolicyType::Automatic);
                scroll.set_min_content_width(228);
                scroll.set_min_content_height(((variants.len()+5)/6*40).min(190) as i32);
                scroll.set_max_content_height(190);scroll.add(&tones);
                box_.pack_start(&scroll,false,false,0);
            }
        }
        let saved=self.state.borrow()["favorites"].as_array().is_some_and(|items|items.contains(&item["text"]));
        let favorite=gtk::Button::with_label(if saved {"Remove from Favourites"} else {"Add to Favourites"});
        let weak=Rc::downgrade(self);let selected=item.clone();
        favorite.connect_clicked(move |_| {if let Some(viewer)=weak.upgrade(){viewer.toggle_favorite(&selected);}});
        box_.pack_start(&favorite,false,false,0);
        let copy=gtk::Button::with_label("Copy");
        let weak=Rc::downgrade(self);let selected=item.clone();
        copy.connect_clicked(move |_| {if let Some(viewer)=weak.upgrade(){viewer.choose(&selected,true);}});
        box_.pack_start(&copy,false,false,0);
        popover.add(&box_);popover.show_all();popover.popup();
        *self.popover.borrow_mut()=Some(popover);
    }

    fn choose_tone(&self,item:&Value) {
        let Some(text)=item["text"].as_str() else {return};
        self.state.borrow_mut()["tones"][base_key(text)]=json!(text);
        let _=save_state(&self.state_path,&self.state.borrow());
        self.choose(item,false);
    }

    fn choose_first(&self) {
        let groups=render_sections(&self.data,&self.state.borrow(),&self.section.borrow(),&self.search.text(),self.expanded.get());
        if let Some(item)=groups.as_array().into_iter().flatten()
            .find_map(|group|group["items"].as_array().and_then(|items|items.first())) {
            self.choose(item,false);
        }
    }

    fn toggle_favorite(self:&Rc<Self>,item:&Value) {
        let Some(text)=item["text"].as_str() else {return};
        let mut state=self.state.borrow_mut();
        let mut favorites=state["favorites"].as_array().cloned().unwrap_or_default();
        if let Some(index)=favorites.iter().position(|value|value.as_str()==Some(text)) {favorites.remove(index);}
        else {favorites.push(json!(text));}
        state["favorites"]=Value::Array(favorites);
        let _=save_state(&self.state_path,&state);
        drop(state);
        if let Some(popover)=self.popover.borrow().as_ref() {popover.popdown();}
        self.activate(item);self.render();
    }

    fn choose(&self,item:&Value,copy_only:bool) {
        let Some(text)=item["text"].as_str() else {return};
        if self.choosing.replace(true) {return;}
        let mut state=self.state.borrow_mut();
        let mut recent=vec![json!(text)];
        recent.extend(state["recent"].as_array().into_iter().flatten().filter(|value|value.as_str()!=Some(text)).take(39).cloned());
        state["recent"]=Value::Array(recent);
        let _=save_state(&self.state_path,&state);
        if self.native {
            let text=text.to_owned();
            let window=self.window.clone();
            let bridge=self.bridge.clone().expect("native viewer has an input bridge");
            let generation=self.generation;
            gtk::glib::timeout_add_local_once(Duration::from_millis(80),move || {
                if copy_only {target::insert_or_copy(&text,None,true);}
                else if !bridge.commit(&text,generation) {
                    target::insert_or_copy(&text,None,true);
                    eprintln!("Original text field is no longer active; character copied");
                }
                gtk::glib::timeout_add_local_once(Duration::from_millis(80),move || {
                    bridge.hide();
                    window.close();
                });
            });
            return;
        }
        self.window.hide();
        let text=text.to_owned();let target=self.target.clone();let window=self.window.clone();
        gtk::glib::timeout_add_local_once(Duration::from_millis(180),move || {
            target::insert_or_copy(&text,target.as_ref(),copy_only);
            window.close();
        });
    }
}

fn main() {
    let mut arguments=std::env::args_os().skip(1);
    let source=arguments.next().expect("Emoji catalogue path is required");
    let flags=arguments.map(|arg|arg.to_string_lossy().into_owned()).collect::<Vec<_>>();
    let daemon_mode=flags.iter().any(|flag|flag=="--daemon");
    let standalone=flags.iter().any(|flag|flag=="--standalone");
    let status=flags.iter().any(|flag|flag=="--status");
    if !daemon_mode && !standalone {
        if let Ok(response)=daemon::request(if status {"status"} else {"show"}) {
            if status {println!("{response}");}
            return;
        }
        if status {println!("{{\"running\":false}}");return;}
    }
    let source=std::fs::read_to_string(source).expect("Emoji catalogue is unavailable");
    let state_root=std::env::var_os("XDG_STATE_HOME").map(PathBuf::from)
        .unwrap_or_else(||PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    let state_path=state_root.join("character-viewer/state.json");
    let data=catalogue(&source);
    let app="lumen-character-viewer";
    gtk::glib::set_prgname(Some(app));
    gtk::glib::set_application_name("Emoji & Symbols");
    if let Err(error)=target::configure_window() {eprintln!("Viewer compositor setup: {error}");}
    if daemon_mode {
        let server=daemon::start().expect("Cannot start character viewer daemon");
        gtk::init().expect("GTK display is unavailable");
        if let (Some(theme),Some(path))=(gtk::IconTheme::default(),std::env::var_os("CHARACTER_VIEWER_ICONS")) {
            theme.append_search_path(&PathBuf::from(path));
        }
        appearance::register_font();
        appearance::install();
        let bridge=match Bridge::load() {Ok(bridge)=>Some(bridge),Err(error)=>{eprintln!("Input method unavailable: {error}");None}};
        let slot:Rc<RefCell<Option<Rc<Viewer>>>>=Rc::new(RefCell::new(None));
        if let Some(input)=bridge.as_ref() {
            let slot=slot.clone();let input_for_change=input.clone();
            input.on_change(move || {
                let current=slot.borrow().as_ref().cloned();
                if let Some(viewer)=current {
                    if viewer.native && (!input_for_change.active() || input_for_change.epoch()!=viewer.generation) {
                        gtk::glib::idle_add_local_once(move ||viewer.close());
                    }
                }
            });
        }
        let socket_path=server.path.clone();
        gtk::glib::timeout_add_local(Duration::from_millis(30),move || {
            for _ in 0..8 {
                match server.listener.accept() {
                    Ok((client,_)) => {
                        let visible=slot.borrow().is_some();
                        let native=slot.borrow().as_ref().is_some_and(|viewer|viewer.native);
                        let available=bridge.as_ref().is_some_and(|bridge|bridge.available());
                        let active=bridge.as_ref().is_some_and(|bridge|bridge.active());
                        let slot=slot.clone();let data=data.clone();let state_path=state_path.clone();
                        let bridge=bridge.clone();
                        daemon::receive(client,move || {
                            let current=slot.borrow().as_ref().cloned();
                            if let Some(window)=current {window.close();}
                            else {
                                let target=target::capture();
                                let viewer=Viewer::new(data.clone(),state_path.clone(),target,Some(slot.clone()),bridge.clone(),false);
                                *slot.borrow_mut()=Some(viewer);
                            }
                            true
                        },available,active,visible,native);
                    }
                    Err(error) if error.kind()==ErrorKind::WouldBlock => break,
                    Err(error) => {eprintln!("Viewer socket: {error}");break;}
                }
            }
            gtk::glib::ControlFlow::Continue
        });
        gtk::main();
        daemon::remove_socket(&socket_path);
    } else {
        let Some(_lock)=daemon::standalone_lock().expect("Cannot lock standalone character viewer") else {return};
        let target=target::capture();
        gtk::init().expect("GTK display is unavailable");
        if let (Some(theme),Some(path))=(gtk::IconTheme::default(),std::env::var_os("CHARACTER_VIEWER_ICONS")) {
            theme.append_search_path(&PathBuf::from(path));
        }
        appearance::register_font();
        appearance::install();
        let _viewer=Viewer::new(data,state_path,target,None,None,false);
        gtk::main();
    }
}
