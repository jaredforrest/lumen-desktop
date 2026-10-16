                  Container(
                    width: sizingInformation.isDesktop ? 180 : 70,
                    color: Theme.of(context).brightness == Brightness.dark ? const Color(0xff242426) : const Color(0xffe8e8ec),
                    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 18),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        if (sizingInformation.isDesktop)
                          const Padding(padding: EdgeInsets.fromLTRB(10, 0, 10, 14),
                            child: Text('LocalSend', style: TextStyle(fontSize: 14, fontWeight: FontWeight.w600))),
                        ...HomeTab.values.map((tab) {
                          final selected = vm.currentTab == tab;
                          return Padding(
                            padding: const EdgeInsets.only(bottom: 4),
                            child: Material(
                              color: selected ? Theme.of(context).colorScheme.primary : Colors.transparent,
                              borderRadius: BorderRadius.circular(7),
                              child: ListTile(
                              dense: true, selected: selected,
                              selectedTileColor: Theme.of(context).colorScheme.primary,
                              selectedColor: Colors.white,
                              shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(7)),
                              contentPadding: const EdgeInsets.symmetric(horizontal: 10),
                              minLeadingWidth: 20, horizontalTitleGap: 10,
                              leading: Icon(tab.icon, size: 20),
                              title: sizingInformation.isDesktop ? Text(tab.label, style: const TextStyle(fontSize: 13)) : null,
                              onTap: () => vm.changeTab(tab),
                              ),
                            ),
                          );
                        }),
                      ],
                    ),
                  ),
