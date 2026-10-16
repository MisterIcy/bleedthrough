<?php

function handle(string $value): void
{
    $object = unserialize($value, ['allowed_classes' => false]);
}
