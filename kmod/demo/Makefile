# The demo plugin's kernel module, as an OpenWrt package. routeh-plugin-build
# builds it from the files under src/ (no PKG_SOURCE_URL), with the
# kernel's make flags of the release's SDK; its version is PKG_VERSION.
include $(TOPDIR)/rules.mk
include $(INCLUDE_DIR)/kernel.mk

PKG_NAME:=demo
PKG_VERSION:=0.1.0
PKG_RELEASE:=1
PKG_LICENSE:=GPL-2.0-only

include $(INCLUDE_DIR)/package.mk

define KernelPackage/demo
  SUBMENU:=Network Devices
  TITLE:=The routeh demo plugin's device kind
  FILES:=$(PKG_BUILD_DIR)/demo.ko
endef

define KernelPackage/demo/description
  A device kind "demo" that drops what is sent through it, like dummy.
endef

define Build/Prepare
	mkdir -p $(PKG_BUILD_DIR)
	$(CP) ./src/* $(PKG_BUILD_DIR)/
endef

define Build/Compile
	+$(KERNEL_MAKE) M="$(PKG_BUILD_DIR)" modules
endef

$(eval $(call KernelPackage,demo))
