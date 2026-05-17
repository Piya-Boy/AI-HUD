/// Windows Terminal tab detector — uses UI Automation to read the
/// currently selected tab name of a wt.exe window. Every tab in
/// Windows Terminal shares the same HWND, so HWND identity alone
/// cannot tell us which tab the user is on. UIA exposes the tab
/// strip as a TabControl with one selected TabItem; reading its
/// Name property is the only reliable way to identify the active tab.
///
/// IUIAutomation is cached per-thread (COM apartment-bound).

#[cfg(target_os = "windows")]
pub use windows_impl::current_selected_tab_name;

#[cfg(not(target_os = "windows"))]
pub use stub::current_selected_tab_name;

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::cell::RefCell;
    use windows::core::{BSTR, VARIANT};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
        UIA_ControlTypePropertyId, UIA_NamePropertyId,
        UIA_SelectionItemIsSelectedPropertyId, UIA_TabItemControlTypeId,
    };

    thread_local! {
        static UIA: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
    }

    fn ensure_uia() -> Option<IUIAutomation> {
        UIA.with(|cell| {
            if let Some(uia) = cell.borrow().as_ref() {
                return Some(uia.clone());
            }
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                let uia: IUIAutomation =
                    CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
                *cell.borrow_mut() = Some(uia.clone());
                Some(uia)
            }
        })
    }

    /// Read the Name of the currently selected TabItem under the given HWND.
    /// Returns None if no tab strip is found, no tab is selected, or UIA fails.
    pub fn current_selected_tab_name(hwnd_value: isize) -> Option<String> {
        if hwnd_value == 0 {
            return None;
        }
        let uia = ensure_uia()?;
        let hwnd = HWND(hwnd_value as *mut _);

        unsafe {
            let root: IUIAutomationElement = uia.ElementFromHandle(hwnd).ok()?;

            // ControlType == TabItem  AND  IsSelected == true
            let v_tabitem: VARIANT = VARIANT::from(UIA_TabItemControlTypeId.0);
            let v_selected: VARIANT = VARIANT::from(true);

            let cond_tabitem = uia
                .CreatePropertyCondition(UIA_ControlTypePropertyId, &v_tabitem)
                .ok()?;
            let cond_selected = uia
                .CreatePropertyCondition(UIA_SelectionItemIsSelectedPropertyId, &v_selected)
                .ok()?;
            let cond = uia.CreateAndCondition(&cond_tabitem, &cond_selected).ok()?;

            let selected_tab = uia
                .ElementFromHandle(hwnd)
                .ok()?
                .FindFirst(TreeScope_Descendants, &cond)
                .ok()?;
            let _ = root;

            let name_var = selected_tab
                .GetCurrentPropertyValue(UIA_NamePropertyId)
                .ok()?;
            let bstr = BSTR::try_from(&name_var).ok()?;
            let s = bstr.to_string();
            if s.is_empty() { None } else { Some(s) }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod stub {
    pub fn current_selected_tab_name(_hwnd_value: isize) -> Option<String> {
        None
    }
}
