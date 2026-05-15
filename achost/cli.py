from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

from .config_merge import format_merge_report, merge_profile_fragments
from .config_rules import get_profile_fragments
from .device_profile import device_default_defconfig, device_default_profiles, load_device_profile
from .kernel_detect import detect_kernel
from .kconfig_inject import (
    format_kconfig_inject_report,
    format_kconfig_rollback_report,
    inject_kconfig_report,
    rollback_kconfig_report,
)
from .docker.moby_check_config import print_human as print_moby_human
from .docker.moby_check_config import print_json as print_moby_json
from .docker.moby_check_config import run_moby_check
from .patch_apply import apply_patch_report, format_apply_report, format_patch_list, list_patch_report
from .report import plan_markdown, to_json, write_plan_reports
from .runtime_install import (
    create_runtime_zip,
    format_runtime_install_report,
    format_runtime_validation_report,
    generate_runtime_package,
    validate_runtime_package,
)
from .runtime_test import build_runtime_test_report, format_runtime_test_report
from .verify_config import evaluate_config, format_human, has_required_failures, summarize_results


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except KeyError as exc:
        print(str(exc), file=sys.stderr)
        return 2
    except (FileNotFoundError, FileExistsError, ValueError) as exc:
        print(str(exc), file=sys.stderr)
        return 2


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="achost")
    subparsers = parser.add_subparsers(dest="command", required=True)

    detect = subparsers.add_parser("detect", help="detect target kernel capabilities")
    detect.add_argument("--kernel-tree", required=True)
    detect.add_argument("--out")
    detect.set_defaults(func=cmd_detect)

    plan = subparsers.add_parser("plan", help="plan config fragments and report gaps")
    plan.add_argument("--kernel-tree", required=True)
    plan.add_argument("--out")
    plan.add_argument("--defconfig")
    plan.add_argument("--profile")
    plan.add_argument("--device")
    plan.add_argument("--json", action="store_true")
    plan.add_argument("--write-report", action="store_true")
    plan.set_defaults(func=cmd_plan)

    verify = subparsers.add_parser("verify-config", help="verify final generated .config")
    verify.add_argument("--config", required=True)
    verify.add_argument("--profile", default="android-container-host-v1")
    verify.add_argument("--json", action="store_true")
    verify.set_defaults(func=cmd_verify_config)

    merge = subparsers.add_parser("merge-fragments", help="merge profile config fragments into a config file")
    merge.add_argument("--base-config", required=True)
    merge.add_argument("--output", required=True)
    merge.add_argument("--profile", default="android-container-host-v1")
    merge.add_argument("--fragment", action="append", dest="fragments")
    merge.add_argument("--json", action="store_true")
    merge.set_defaults(func=cmd_merge_fragments)

    inject = subparsers.add_parser("inject-kconfig", help="dry-run or inject ACHKL Kconfig into a target tree")
    inject.add_argument("--kernel-tree", required=True)
    inject.add_argument("--destination", default="vendor/android-container-host/AndroidContainerHost.Kconfig")
    inject_mode = inject.add_mutually_exclusive_group()
    inject_mode.add_argument("--dry-run", action="store_true")
    inject_mode.add_argument("--apply", action="store_true")
    inject.add_argument("--json", action="store_true")
    inject.set_defaults(func=cmd_inject_kconfig)

    rollback = subparsers.add_parser("rollback-kconfig", help="dry-run or remove ACHKL Kconfig injection from a target tree")
    rollback.add_argument("--kernel-tree", required=True)
    rollback.add_argument("--destination", default="vendor/android-container-host/AndroidContainerHost.Kconfig")
    rollback_mode = rollback.add_mutually_exclusive_group()
    rollback_mode.add_argument("--dry-run", action="store_true")
    rollback_mode.add_argument("--apply", action="store_true")
    rollback.add_argument("--json", action="store_true")
    rollback.set_defaults(func=cmd_rollback_kconfig)

    moby = subparsers.add_parser("verify-moby-check-config", help="run and classify Docker/Moby check-config output")
    moby.add_argument("--script", required=True)
    moby.add_argument("--config", required=True)
    moby.add_argument("--json", action="store_true")
    moby.set_defaults(func=cmd_verify_moby_check_config)

    list_patches = subparsers.add_parser("list-patches", help="list available patch modules")
    list_patches.add_argument("--kernel-tree", required=True)
    list_patches.add_argument("--kernel-version")
    list_patches.add_argument("--json", action="store_true")
    list_patches.set_defaults(func=cmd_list_patches)

    apply_patches = subparsers.add_parser("apply-patches", help="dry-run or apply patch modules")
    apply_patches.add_argument("--kernel-tree", required=True)
    apply_patches.add_argument("--kernel-version")
    apply_patches.add_argument("--patch", action="append", dest="patches")
    mode = apply_patches.add_mutually_exclusive_group()
    mode.add_argument("--dry-run", action="store_true")
    mode.add_argument("--apply", action="store_true")
    apply_patches.add_argument("--json", action="store_true")
    apply_patches.set_defaults(func=cmd_apply_patches)

    runtime_install = subparsers.add_parser("runtime-install", help="generate Android runtime install package")
    runtime_install.add_argument("--output", required=True)
    runtime_install.add_argument("--mode", choices=("manual", "kernelsu-module"), default="manual")
    runtime_install.add_argument("--module-target", choices=("legacy", "base", "docker", "lxc"), default="legacy")
    runtime_install.add_argument("--cgroup-mode", choices=("v1", "v2"), default="v1")
    runtime_install.add_argument("--docker-runtime-mode", choices=("native",), default="native")
    runtime_install.add_argument("--docker-asset")
    runtime_install.add_argument("--docker-sha256")
    runtime_install.add_argument("--compose-asset")
    runtime_install.add_argument("--compose-sha256")
    runtime_install.add_argument("--buildx-asset")
    runtime_install.add_argument("--buildx-sha256")
    runtime_install.add_argument("--buildkit-asset")
    runtime_install.add_argument("--buildkit-sha256")
    runtime_install.add_argument("--lxc-asset")
    runtime_install.add_argument("--lxc-sha256")
    runtime_install.add_argument("--start-docker-on-boot", action="store_true")
    runtime_install.add_argument("--zip", dest="zip_output", nargs="?", const="auto")
    runtime_install.add_argument("--json", action="store_true")
    runtime_install.set_defaults(func=cmd_runtime_install)

    runtime_validate = subparsers.add_parser("runtime-validate", help="validate a generated runtime package")
    runtime_validate.add_argument("--package-root", required=True)
    runtime_validate.add_argument("--module-target", choices=("base", "docker", "lxc"), required=True)
    runtime_validate.add_argument("--zip", dest="zip_output")
    runtime_validate.add_argument("--release", action="store_true")
    runtime_validate.add_argument("--json", action="store_true")
    runtime_validate.set_defaults(func=cmd_runtime_validate)

    runtime_test = subparsers.add_parser("runtime-test", help="print Android runtime test command")
    runtime_test.add_argument("--package-root", default="/data/adb/achost")
    runtime_test.add_argument("--target", choices=("all", "network", "docker", "lxc"), default="all")
    runtime_test.add_argument("--out-dir", default="/data/local/tmp/achost-runtime-test")
    runtime_test.add_argument("--json", action="store_true")
    runtime_test.set_defaults(func=cmd_runtime_test)

    return parser


def cmd_detect(args: argparse.Namespace) -> int:
    result = detect_kernel(args.kernel_tree, args.out)
    print(to_json(result))
    return 0


def cmd_plan(args: argparse.Namespace) -> int:
    device = load_device_profile(args.device) if args.device else None
    profile = args.profile or device_default_profiles(device) or "android-container-host-v1"
    defconfig = args.defconfig or device_default_defconfig(device)
    detect = detect_kernel(args.kernel_tree, args.out)
    config_path = detect.get("generated_config")
    config_results = evaluate_config(config_path, profile) if config_path else []
    plan = build_plan(detect, config_results, profile, defconfig, device)

    if args.write_report:
        report_dir = Path(detect["out"]) / "achost"
        plan["reports"] = {
            "json": str(report_dir / "plan.json"),
            "markdown": str(report_dir / "plan.md"),
        }
        write_plan_reports(detect["out"], plan)

    if args.json:
        print(to_json(plan))
    else:
        print(plan_markdown(plan), end="")
        if args.write_report:
            print(f"Reports: {plan['reports']['markdown']} {plan['reports']['json']}")
    return 1 if has_required_failures(config_results) else 0


def cmd_verify_config(args: argparse.Namespace) -> int:
    config = Path(args.config).expanduser().resolve()
    if not config.exists():
        raise FileNotFoundError(f"config not found: {config}")

    results = evaluate_config(config, args.profile)
    payload = {
        "config": str(config),
        "profile": args.profile,
        "summary": summarize_results(results),
        "results": results,
    }

    if args.json:
        print(to_json(payload))
    else:
        print(format_human(results))
    return 1 if has_required_failures(results) else 0


def cmd_merge_fragments(args: argparse.Namespace) -> int:
    report = merge_profile_fragments(args.base_config, args.output, profile=args.profile, extra_fragments=args.fragments)
    if args.json:
        print(to_json(report))
    else:
        print(format_merge_report(report))
    return 0


def cmd_inject_kconfig(args: argparse.Namespace) -> int:
    report = inject_kconfig_report(args.kernel_tree, destination=args.destination, apply=args.apply)
    if args.json:
        print(to_json(report))
    else:
        print(format_kconfig_inject_report(report))
    return 0


def cmd_rollback_kconfig(args: argparse.Namespace) -> int:
    report = rollback_kconfig_report(args.kernel_tree, destination=args.destination, apply=args.apply)
    if args.json:
        print(to_json(report))
    else:
        print(format_kconfig_rollback_report(report))
    return 0


def cmd_verify_moby_check_config(args: argparse.Namespace) -> int:
    report = run_moby_check(args.script, args.config)
    if args.json:
        print(print_moby_json(report))
    else:
        print(print_moby_human(report))
    return report["exit_code"]


def cmd_list_patches(args: argparse.Namespace) -> int:
    report = list_patch_report(args.kernel_tree, args.kernel_version)
    if args.json:
        print(to_json(report))
    else:
        print(format_patch_list(report))
    return 0


def cmd_apply_patches(args: argparse.Namespace) -> int:
    report = apply_patch_report(
        args.kernel_tree,
        kernel_version=args.kernel_version,
        patch_names=args.patches,
        apply=args.apply,
    )
    if args.json:
        print(to_json(report))
    else:
        print(format_apply_report(report))
    return 0 if report["ok"] else 1


def cmd_runtime_install(args: argparse.Namespace) -> int:
    report = generate_runtime_package(
        args.output,
        mode=args.mode,
        cgroup_mode=args.cgroup_mode,
        docker_asset=args.docker_asset,
        docker_sha256=args.docker_sha256,
        compose_asset=args.compose_asset,
        compose_sha256=args.compose_sha256,
        buildx_asset=args.buildx_asset,
        buildx_sha256=args.buildx_sha256,
        buildkit_asset=args.buildkit_asset,
        buildkit_sha256=args.buildkit_sha256,
        lxc_asset=args.lxc_asset,
        lxc_sha256=args.lxc_sha256,
        start_docker_on_boot=args.start_docker_on_boot,
        docker_runtime_mode=args.docker_runtime_mode,
        module_target=args.module_target,
    )
    if args.zip_output is not None:
        zip_output = None if args.zip_output == "auto" else args.zip_output
        report["zip"] = str(create_runtime_zip(args.output, zip_output))
    if args.json:
        print(to_json(report))
    else:
        print(format_runtime_install_report(report))
    return 0


def cmd_runtime_validate(args: argparse.Namespace) -> int:
    report = validate_runtime_package(
        args.package_root,
        args.module_target,
        zip_path=args.zip_output,
        release=args.release,
    )
    if args.json:
        print(to_json(report))
    else:
        print(format_runtime_validation_report(report))
    return 0


def cmd_runtime_test(args: argparse.Namespace) -> int:
    report = build_runtime_test_report(args.package_root, target=args.target, out_dir=args.out_dir)
    if args.json:
        print(to_json(report))
    else:
        print(format_runtime_test_report(report))
    return 0


def build_plan(
    detect: dict[str, Any],
    config_results: list[dict[str, Any]],
    profile: str,
    defconfig: str | None,
    device: dict[str, Any] | None = None,
) -> dict[str, Any]:
    risks = list(detect.get("risk", []))
    if defconfig and defconfig not in detect.get("defconfig_candidates", []):
        risks.append(f"requested defconfig {defconfig} was not found in detected candidates")

    missing_required = [item["symbol"] for item in config_results if item["level"] == "required" and not item["ok"]]
    missing_recommended = [item["symbol"] for item in config_results if item["level"] == "recommended" and not item["ok"]]

    return {
        "profile": profile,
        "defconfig": defconfig,
        "device": device,
        "detect": detect,
        "fragments": get_profile_fragments(profile),
        "config_summary": summarize_results(config_results),
        "config_results": config_results,
        "missing_required": missing_required,
        "missing_recommended": missing_recommended,
        "risks": risks,
        "notes": [
            "Phase 1 only reports config/source gaps and does not modify the target kernel tree.",
            "qtaguid and cgroup noprefix findings are risks for later patch/runtime phases, not automatic fixes.",
        ],
    }


if __name__ == "__main__":
    raise SystemExit(main())
