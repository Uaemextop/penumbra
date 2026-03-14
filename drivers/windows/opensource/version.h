/*
 * version.h
 *
 * Version constants for the MediaTek USB CDC ACM to Serial Port KMDF driver.
 * Shared between mtk_usb2ser.rc (VERSIONINFO) and source code.
 *
 * SPDX-License-Identifier: MIT
 */

#pragma once

#define VER_MAJOR       2
#define VER_MINOR       0
#define VER_PATCH       0
#define VER_BUILD       0

#define VER_FILEVERSION             VER_MAJOR, VER_MINOR, VER_PATCH, VER_BUILD
#define VER_PRODUCTVERSION          VER_MAJOR, VER_MINOR, VER_PATCH, VER_BUILD

#define VER_FILEVERSION_STR         "2.0.0.0"
#define VER_PRODUCTVERSION_STR      "2.0.0.0"

#define VER_COMPANYNAME_STR         "MTK Loader Drivers Opensource"
#define VER_FILEDESCRIPTION_STR     "MediaTek USB CDC ACM Serial Port Driver (KMDF)"
#define VER_INTERNALNAME_STR        "mtk_usb2ser.sys"
#define VER_LEGALCOPYRIGHT_STR      "Copyright (C) 2026 MTK Loader Drivers Opensource. MIT License."
#define VER_ORIGINALFILENAME_STR    "mtk_usb2ser.sys"
#define VER_PRODUCTNAME_STR         "MediaTek USB CDC ACM Driver"
