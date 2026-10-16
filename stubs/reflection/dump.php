<?php

/*
 * Reflection dump for Bleedthrough's built-in data.
 *
 * Prints, as canonical JSON on stdout, the facts that PHP's own Reflection reports about the
 * internal functions, classes, interfaces, traits, enums and constants of the running interpreter.
 * Facts only: names, types, flags and values. No documentation text, no file paths, nothing that
 * depends on the machine except what the interpreter itself reports.
 *
 * Run (see `cargo xtask refresh-reflection`):  php dump.php
 * The environment variable BT_IMAGE, when set, is recorded as the image the script ran in.
 *
 * The script only reads: it declares no function or class of its own beyond the helpers below,
 * writes no file, opens no connection and runs no code from the interpreter's extensions.
 */

error_reporting(E_ALL);
ini_set('display_errors', 'stderr');

/** The text of a type, or '' when there is none. */
function bt_type(?ReflectionType $type): string
{
    return $type === null ? '' : (string) $type;
}

/** A string as a PHP literal on one line: single-quoted when printable ASCII, else double-quoted with escapes. */
function bt_string(string $value): string
{
    if (preg_match('/^[\x20-\x7e]*$/D', $value) === 1) {
        return "'" . addcslashes($value, "'\\") . "'";
    }
    $escapes = ["\n" => '\n', "\r" => '\r', "\t" => '\t', "\v" => '\v', "\e" => '\e', "\f" => '\f', '\\' => '\\\\', '"' => '\\"', '$' => '\\$'];
    $out = '';
    foreach (str_split($value) as $byte) {
        $code = ord($byte);
        $out .= $escapes[$byte] ?? ($code >= 0x20 && $code <= 0x7e ? $byte : sprintf('\\x%02X', $code));
    }
    return '"' . $out . '"';
}

/**
 * A constant's value as PHP literal text on one line, with no tab or newline in it; empty for a value that
 * has no literal form (a resource or an object).
 */
function bt_value(mixed $value): string
{
    if ($value === null) {
        return 'null';
    }
    if (is_bool($value)) {
        return $value ? 'true' : 'false';
    }
    if (is_int($value)) {
        return (string) $value;
    }
    if (is_float($value)) {
        return var_export($value, true);
    }
    if (is_string($value)) {
        return bt_string($value);
    }
    if (is_array($value)) {
        $items = [];
        $list = array_is_list($value);
        foreach ($value as $key => $item) {
            $text = bt_value($item);
            if ($text === '') {
                return '';
            }
            $items[] = $list ? $text : bt_value($key) . ' => ' . $text;
        }
        return '[' . implode(', ', $items) . ']';
    }
    return '';
}

/** @return list<array<string, mixed>> */
function bt_params(ReflectionFunctionAbstract $function): array
{
    $params = [];
    foreach ($function->getParameters() as $p) {
        $params[] = [
            'name' => $p->getName(),
            'type' => bt_type($p->getType()),
            'by_ref' => $p->isPassedByReference(),
            'optional' => $p->isOptional(),
            'variadic' => $p->isVariadic(),
            'has_default' => $p->isDefaultValueAvailable(),
        ];
    }
    return $params;
}

/** Version-dependent Reflection methods are guarded: the script runs on PHP 8.0 to 8.5. */

/** @return array<string, mixed> */
function bt_signature(ReflectionFunctionAbstract $function): array
{
    return [
        'params' => bt_params($function),
        'return_type' => bt_type($function->getReturnType()),
        'tentative_return_type' => method_exists($function, 'getTentativeReturnType') ? bt_type($function->getTentativeReturnType()) : '',
        'returns_ref' => $function->returnsReference(),
    ];
}

function bt_visibility(object $member): string
{
    return $member->isPrivate() ? 'private' : ($member->isProtected() ? 'protected' : 'public');
}

/** @return array<string, mixed> */
function bt_class(ReflectionClass $class): array
{
    $is_enum = method_exists($class, 'isEnum') && $class->isEnum();
    $kind = $class->isInterface() ? 'interface' : ($class->isTrait() ? 'trait' : ($is_enum ? 'enum' : 'class'));
    $parent = $class->getParentClass();
    $out = [
        'kind' => $kind,
        'extension' => $class->getExtensionName() ?: '',
        'abstract' => $kind === 'class' && $class->isAbstract(),
        'final' => $class->isFinal(),
        'readonly' => method_exists($class, 'isReadOnly') && $class->isReadOnly(),
        'parent' => $parent ? $parent->getName() : '',
        'interfaces' => $class->getInterfaceNames(),
        'backing_type' => '',
        'constants' => [],
        'cases' => [],
        'properties' => [],
        'methods' => [],
    ];
    sort($out['interfaces']);
    if ($is_enum) {
        $out['backing_type'] = bt_type((new ReflectionEnum($class->getName()))->getBackingType());
    }
    foreach ($class->getReflectionConstants() as $constant) {
        if ($constant->getDeclaringClass()->getName() !== $class->getName()) {
            continue;
        }
        if (method_exists($constant, 'isEnumCase') && $constant->isEnumCase()) {
            $case = $constant->getValue();
            $out['cases'][$constant->getName()] = $case instanceof BackedEnum ? bt_value($case->value) : '';
            continue;
        }
        $out['constants'][$constant->getName()] = [
            'value' => bt_value($constant->getValue()),
            'type' => method_exists($constant, 'getType') ? bt_type($constant->getType()) : '',
            'visibility' => bt_visibility($constant),
            'final' => method_exists($constant, 'isFinal') && $constant->isFinal(),
        ];
    }
    foreach ($class->getProperties() as $property) {
        if ($property->getDeclaringClass()->getName() !== $class->getName()) {
            continue;
        }
        $out['properties'][$property->getName()] = [
            'type' => bt_type($property->getType()),
            'visibility' => bt_visibility($property),
            'static' => $property->isStatic(),
            'readonly' => method_exists($property, 'isReadOnly') && $property->isReadOnly(),
            'has_default' => $property->hasDefaultValue(),
        ];
    }
    foreach ($class->getMethods() as $method) {
        if ($method->getDeclaringClass()->getName() !== $class->getName()) {
            continue;
        }
        $out['methods'][$method->getName()] = bt_signature($method) + [
            'visibility' => bt_visibility($method),
            'static' => $method->isStatic(),
            'final' => $method->isFinal(),
            'abstract' => $method->isAbstract(),
        ];
    }
    ksort($out['constants']);
    ksort($out['cases']);
    ksort($out['properties']);
    ksort($out['methods']);
    return $out;
}

$functions = [];
foreach (get_defined_functions()['internal'] as $name) {
    $function = new ReflectionFunction($name);
    $functions[$name] = bt_signature($function) + ['extension' => $function->getExtensionName() ?: ''];
}
ksort($functions);

$classes = [];
foreach (array_merge(get_declared_classes(), get_declared_interfaces(), get_declared_traits()) as $name) {
    $class = new ReflectionClass($name);
    if ($class->isInternal()) {
        $classes[$class->getName()] = bt_class($class);
    }
}
ksort($classes);

$constants = [];
foreach (get_defined_constants(true) as $group => $values) {
    if ($group === 'user') {
        continue;
    }
    foreach ($values as $name => $value) {
        $constants[$name] = [
            'extension' => (string) $group,
            'type' => get_debug_type($value),
            'value' => bt_value($value),
        ];
    }
}
ksort($constants);

$extensions = get_loaded_extensions();
sort($extensions);

/** One JSON value on one line. */
function bt_json(mixed $value): string
{
    return json_encode(
        $value,
        JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_PRESERVE_ZERO_FRACTION | JSON_INVALID_UTF8_SUBSTITUTE | JSON_THROW_ON_ERROR,
    );
}

/** A map with one symbol per line, so that a change to one symbol is one changed line. */
function bt_section(string $name, array $symbols): string
{
    $lines = [];
    foreach ($symbols as $key => $value) {
        $lines[] = bt_json((string) $key) . ':' . bt_json($value);
    }
    return bt_json($name) . ":{\n" . implode(",\n", $lines) . "\n}";
}

echo "{\n",
    '"php_version":', bt_json(PHP_VERSION), ",\n",
    '"image":', bt_json(getenv('BT_IMAGE') ?: 'local'), ",\n",
    '"extensions":', bt_json($extensions), ",\n",
    bt_section('functions', $functions), ",\n",
    bt_section('classes', $classes), ",\n",
    bt_section('constants', $constants), "\n}\n";
