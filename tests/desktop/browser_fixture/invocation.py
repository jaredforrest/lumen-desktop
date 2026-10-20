"""Explicit private-profile application invocation; no consent-bypass options."""
from pathlib import Path
from urllib.parse import urlsplit

EXCLUDED_SETUP=('first-run onboarding','default-browser startup check')

def command(browser,profile,url,mode='normal',accessibility=False):
    browser,profile=Path(browser),Path(profile)
    if not browser.is_absolute() or not str(browser).startswith('/nix/store/') or '..' in browser.parts:raise ValueError('Explicit immutable browser executable required')
    if not profile.is_absolute() or not profile.is_relative_to('/home/fixture') or '..' in profile.parts:raise ValueError('Explicit private profile beneath fixture HOME required')
    address=urlsplit(url)
    if address.scheme!='http' or address.hostname!='127.0.0.1' or address.username is not None or address.password is not None or address.port is None:raise ValueError('Synthetic loopback origin required')
    if mode not in ('normal','app','private'):raise ValueError('Unsupported explicit browser surface mode')
    result=[str(browser),'--user-data-dir='+str(profile),'--no-first-run','--no-default-browser-check','--ozone-platform=wayland','--remote-debugging-pipe']
    if accessibility:result.append('--force-renderer-accessibility')
    if mode=='private':result.append('--incognito')
    result.append('--app='+url if mode=='app' else url)
    return result
