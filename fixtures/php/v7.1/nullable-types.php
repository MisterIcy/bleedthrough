<?php

function find(?int $id): ?string
{
    return $id === null ? null : 'x';
}
