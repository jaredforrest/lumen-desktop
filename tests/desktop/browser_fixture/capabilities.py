"""Per-action application observations; one passing action never closes a whole row."""
ACTIONS={
 'APP-BROWSE-01':('new_tab','close_tab','reopen_tab','pin_tab','reorder_tab','detach_tab','duplicate_tab','tab_groups','close_last_tab_policy'),
 'APP-BROWSE-02':('edit_url','select_url','copy_url','paste_url','navigate','stop','reload','back','forward','draft_focus_preserved','offline_retry','invalid_url_edit'),
 'APP-BROWSE-03':('bookmark_add','bookmark_edit','bookmark_delete','bookmark_folders','bookmark_search','bookmark_import','bookmark_export','history_search','history_delete','private_history_isolation','import_duplicates'),
 'APP-BROWSE-04':('running_download','completed_download_hash','pause_download','resume_download','cancel_download','reveal_download','open_download','retry_download','missing_download_recovery','downloads_popover','downloads_manager'),
 'APP-BROWSE-05':tuple(capability+'_'+action for capability in ('notifications','location','camera','microphone','clipboard','capture') for action in ('allow','deny','revoke'))+('capture_picker_target',),
 'APP-BROWSE-06':('new_profile','private_window','guest_window','cookie_isolation','history_isolation','extension_isolation','private_surface_label'),
 'APP-BROWSE-07':('extension_load','extension_disable','extension_remove','toolbar_overflow','extension_popup','extension_frame_parent'),
 'APP-BROWSE-08':('reader_availability','print_parent','print_to_file','pdf_controls','zoom_in','zoom_reset','find','page_vs_output_scale'),
 'APP-BROWSE-09':('app_mode','picture_in_picture','browser_fullscreen','page_fullscreen','escape_fullscreen','resume_page_input'),
 'APP-BROWSE-10':('default_handler','external_link_profile','protocol_consent','file_links','literal_protocol_arguments'),
}

class Inventory:
 def __init__(self,version):
  self.version=version;self.results={key:{} for key in ACTIONS}
 def record(self,capability,action,status,evidence,reason=''):
  if capability not in ACTIONS or action not in ACTIONS[capability]:raise ValueError('Unknown application action')
  if status not in ('verified_compatible','dependency_blocked','implemented_unverified'):raise ValueError('Observed action requires a supported conformance status')
  if not evidence or (status=='dependency_blocked' and not reason):raise ValueError('Observed status requires concrete evidence and blocker details')
  if action in self.results[capability]:raise ValueError('Action already has an immutable observation')
  self.results[capability][action]=dict(status=status,evidence=evidence,reason=reason)
 def snapshot(self):
  return [dict(capability_id=key,provider_app_id='brave-browser',provider_version=self.version,supported_actions=[action for action,value in self.results[key].items() if value['status']=='verified_compatible'],unsupported_actions=[],backend='actual retained browser, private profile and synthetic loopback provider',permission_set='Actual origin permissions; no diagnostic permission overrides',offline_policy='Network namespace has no external route',scenario_results=self.results[key],remaining_actions=[action for action in actions if action not in self.results[key]],reference_evidence=[],compatibility_differences=['Reference measurements unavailable; application compatibility only'],status='verified_compatible' if len(self.results[key])==len(actions) and all(value['status']=='verified_compatible' for value in self.results[key].values()) else 'unassessed') for key,actions in ACTIONS.items()]
