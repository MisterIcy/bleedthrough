<?php

function handle(string $value): void
{
    $object = unserialize($value);
}

handle($_GET['x']);
